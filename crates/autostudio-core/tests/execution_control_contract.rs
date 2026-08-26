use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use autostudio_core::agent::AgentRunId;
use autostudio_core::execution_control::{
    ApprovalGrant, ApprovalGrantDraft, ApprovalGrantId, ApprovalSubject, ExecutionControl,
    ExecutionControlCommand, ExecutionControlError, ExecutionControlManager,
    ExecutionControlManagerError, ExecutionControlSnapshot, ExecutionControlStore,
    ExecutionControlStoreError, ExecutionReservationId, InferenceBudgetCharge, Money,
    ReservationStatus, RunBudgetDimension, RunBudgetLimits, RunBudgetLimitsDraft, SideEffectClass,
    ToolBudgetCharge, ToolExecutionClaim, ToolExecutionClaimDraft, ToolResourceDimension,
    ToolResourceLimit, ToolResourceLimitDraft, ToolResourceUsage, ToolSettlement,
};
use sha2::{Digest, Sha256};

#[test]
fn grant_budget_reservation_settlement_and_restore_are_durable_and_idempotent() {
    let run_id = AgentRunId::new();
    let grant_id = ApprovalGrantId::new();
    let mut control = control(run_id.clone(), limits(8, 8, 10_000, 1_000, 2));
    control
        .issue_grant(grant(
            grant_id.clone(),
            run_id.clone(),
            42,
            vec!["project".to_owned(), "track:piano".to_owned()],
            8,
            500,
            None,
        ))
        .expect("Grant issued");
    let inference_usage = control
        .record_inference(
            "turn-1",
            InferenceBudgetCharge {
                input_tokens: 100,
                output_tokens: 50,
                wall_clock_millis: 100,
                cost: usd(10),
            },
            1_010,
        )
        .expect("Inference charged");
    assert_eq!(inference_usage.inference_turns(), 1);
    assert_eq!(inference_usage.tokens(), 150);

    let reservation_id = ExecutionReservationId::new();
    let claim = claim(
        grant_id,
        run_id,
        42,
        vec!["track:piano".to_owned()],
        ToolBudgetCharge {
            side_effects: 4,
            preview_renders: 1,
            asset_bytes: 512,
            wall_clock_millis: 1_000,
            cost: usd(50),
        },
        resource_usage(128, 1, 500, 1_024, 512, 1_000),
    );
    let reserved = control
        .authorize_tool(
            reservation_id.clone(),
            claim.clone(),
            resource_limit(),
            1_020,
        )
        .expect("Tool authorized");
    assert_eq!(reserved.status(), ReservationStatus::Reserved);
    assert_eq!(control.usage().unwrap().tool_executions(), 1);
    assert_eq!(control.usage().unwrap().concurrent_tools(), 1);

    let replay = control
        .authorize_tool(reservation_id.clone(), claim, resource_limit(), 1_999)
        .expect("matching authorization replay");
    assert_eq!(replay, reserved);
    assert_eq!(control.usage().unwrap().tool_executions(), 1);

    let settlement = ToolSettlement {
        budget_charge: ToolBudgetCharge {
            side_effects: 3,
            preview_renders: 1,
            asset_bytes: 400,
            wall_clock_millis: 800,
            cost: usd(30),
        },
        resources: resource_usage(128, 1, 300, 900, 400, 800),
    };
    let settled = control
        .settle_tool(&reservation_id, settlement.clone(), 1_800)
        .expect("Tool settled");
    assert_eq!(settled.status(), ReservationStatus::Completed);
    let replay = control
        .settle_tool(&reservation_id, settlement, 2_500)
        .expect("matching settlement replay");
    assert_eq!(replay, settled);
    let usage = control.usage().expect("usage");
    assert_eq!(usage.concurrent_tools(), 0);
    assert_eq!(usage.peak_concurrent_tools(), 1);
    assert_eq!(usage.side_effects(), 3);
    assert_eq!(usage.preview_renders(), 1);
    assert_eq!(usage.asset_bytes(), 400);
    assert_eq!(usage.cost().minor_units(), 40);

    let serialized = serde_json::to_string(&control).expect("serialize ledger");
    let restored: ExecutionControl = serde_json::from_str(&serialized).expect("restore ledger");
    restored.validate_restored().expect("valid restored ledger");
    assert_eq!(restored, control);
}

#[test]
fn approval_grant_rejects_revision_tool_target_effect_cost_and_expiry_escalation() {
    let run_id = AgentRunId::new();
    let grant_id = ApprovalGrantId::new();
    let mut control = control(run_id.clone(), limits(8, 8, 10_000, 1_000, 2));
    control
        .issue_grant(grant(
            grant_id.clone(),
            run_id.clone(),
            42,
            vec!["track:piano".to_owned()],
            4,
            100,
            Some(1_500),
        ))
        .expect("Grant issued");

    let wrong_revision = claim(
        grant_id.clone(),
        run_id.clone(),
        43,
        vec!["track:piano".to_owned()],
        charge(1, 10),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, wrong_revision, 1_100),
        Err(ExecutionControlError::GrantBindingMismatch)
    );

    let wrong_target = claim(
        grant_id.clone(),
        run_id.clone(),
        42,
        vec!["track:bass".to_owned()],
        charge(1, 10),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, wrong_target, 1_100),
        Err(ExecutionControlError::GrantTargetExceeded)
    );

    let too_many_effects = claim(
        grant_id.clone(),
        run_id.clone(),
        42,
        vec!["track:piano".to_owned()],
        charge(5, 10),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, too_many_effects, 1_100),
        Err(ExecutionControlError::GrantEffectExceeded)
    );

    let too_expensive = claim(
        grant_id.clone(),
        run_id.clone(),
        42,
        vec!["track:piano".to_owned()],
        charge(1, 101),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, too_expensive, 1_100),
        Err(ExecutionControlError::GrantCostExceeded)
    );

    let expired = claim(
        grant_id,
        run_id,
        42,
        vec!["track:piano".to_owned()],
        charge(1, 10),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, expired, 1_500),
        Err(ExecutionControlError::GrantExpired)
    );
}

#[test]
fn grant_budget_and_tool_resource_limits_fail_as_three_distinct_controls() {
    let run_id = AgentRunId::new();
    let grant_id = ApprovalGrantId::new();
    let configured = limits(2, 4, 150, 1_000, 1);
    let mut control = control(run_id.clone(), configured);
    control
        .issue_grant(grant(
            grant_id.clone(),
            run_id.clone(),
            42,
            vec!["track:piano".to_owned()],
            20,
            900,
            None,
        ))
        .expect("Grant issued");

    assert_eq!(
        control.record_inference(
            "turn-over-budget",
            InferenceBudgetCharge {
                input_tokens: 100,
                output_tokens: 51,
                wall_clock_millis: 100,
                cost: usd(1),
            },
            1_010,
        ),
        Err(ExecutionControlError::RunBudgetExceeded {
            dimension: RunBudgetDimension::Tokens
        })
    );
    assert_eq!(control.usage().unwrap().inference_turns(), 0);

    let excessive_resource = claim(
        grant_id.clone(),
        run_id.clone(),
        42,
        vec!["track:piano".to_owned()],
        charge(1, 10),
        resource_usage(2_001, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        control.authorize_tool(
            ExecutionReservationId::new(),
            excessive_resource,
            resource_limit(),
            1_020,
        ),
        Err(ExecutionControlError::ToolResourceExceeded {
            dimension: ToolResourceDimension::InputBytes
        })
    );
    assert_eq!(control.usage().unwrap().tool_executions(), 0);

    let first = claim(
        grant_id.clone(),
        run_id.clone(),
        42,
        vec!["track:piano".to_owned()],
        charge(1, 10),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    let first_id = ExecutionReservationId::new();
    control
        .authorize_tool(first_id.clone(), first, resource_limit(), 1_030)
        .expect("first Tool reserved");
    let second = claim(
        grant_id,
        run_id,
        42,
        vec!["track:piano".to_owned()],
        charge(1, 10),
        resource_usage(1, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        control.authorize_tool(
            ExecutionReservationId::new(),
            second.clone(),
            resource_limit(),
            1_040,
        ),
        Err(ExecutionControlError::RunBudgetExceeded {
            dimension: RunBudgetDimension::ConcurrentTools
        })
    );
    control
        .cancel_tool(&first_id, 1_050)
        .expect("reservation cancelled");
    control
        .authorize_tool(
            ExecutionReservationId::new(),
            second,
            resource_limit(),
            1_060,
        )
        .expect("concurrency returned after cancellation");
}

#[test]
fn authorization_rejects_in_grant_budget_then_resource_order() {
    let run_id = AgentRunId::new();
    let grant_id = ApprovalGrantId::new();
    let mut control = control(run_id.clone(), limits(2, 4, 150, 1_000, 1));
    control
        .issue_grant(grant(
            grant_id.clone(),
            run_id.clone(),
            42,
            vec!["track:piano".to_owned()],
            20,
            900,
            None,
        ))
        .expect("Grant issued");

    let grant_and_resource_violation = claim(
        grant_id.clone(),
        run_id.clone(),
        42,
        vec!["track:bass".to_owned()],
        charge(1, 10),
        resource_usage(2_001, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, grant_and_resource_violation, 1_005),
        Err(ExecutionControlError::GrantTargetExceeded)
    );

    control
        .authorize_tool(
            ExecutionReservationId::new(),
            claim(
                grant_id.clone(),
                run_id.clone(),
                42,
                vec!["track:piano".to_owned()],
                charge(1, 10),
                resource_usage(1, 1, 1, 1, 1, 1),
            ),
            resource_limit(),
            1_010,
        )
        .expect("first Tool reserved");
    let budget_and_resource_violation = claim(
        grant_id,
        run_id,
        42,
        vec!["track:piano".to_owned()],
        charge(1, 10),
        resource_usage(2_001, 1, 1, 1, 1, 1),
    );
    assert_eq!(
        authorize(&mut control, budget_and_resource_violation, 1_015),
        Err(ExecutionControlError::RunBudgetExceeded {
            dimension: RunBudgetDimension::ConcurrentTools
        })
    );
}

#[test]
fn creator_configuration_cannot_raise_the_system_ceiling_and_corruption_fails_closed() {
    let run_id = AgentRunId::new();
    let configured = limits(9, 4, 1_000, 100, 1);
    let ceiling = limits(8, 4, 1_000, 100, 1);
    assert_eq!(
        ExecutionControl::new(run_id.clone(), configured, ceiling, 1_000),
        Err(
            ExecutionControlError::ConfiguredBudgetExceedsSystemCeiling {
                dimension: RunBudgetDimension::InferenceTurns
            }
        )
    );

    let control = control(run_id, limits(8, 4, 1_000, 100, 1));
    let mut value = serde_json::to_value(&control).expect("serialize ledger");
    value["formatRevision"] = serde_json::Value::String("unknown-format".to_owned());
    let restored: ExecutionControl = serde_json::from_value(value).expect("deserialize shape");
    assert_eq!(
        restored.validate_restored(),
        Err(ExecutionControlError::CorruptRestoredState)
    );
}

#[test]
fn restored_ledger_rejects_a_grant_rewritten_below_its_reserved_scope() {
    let run_id = AgentRunId::new();
    let grant_id = ApprovalGrantId::new();
    let mut control = control(run_id.clone(), limits(8, 4, 1_000, 100, 1));
    control
        .issue_grant(grant(
            grant_id.clone(),
            run_id.clone(),
            42,
            vec!["track:piano".to_owned()],
            4,
            100,
            None,
        ))
        .expect("Grant issued");
    authorize(
        &mut control,
        claim(
            grant_id,
            run_id,
            42,
            vec!["track:piano".to_owned()],
            charge(4, 10),
            resource_usage(1, 1, 1, 1, 1, 1),
        ),
        1_100,
    )
    .expect("full Grant scope reserved");

    let mut value = serde_json::to_value(&control).expect("serialize ledger");
    value["grants"][0]["maxEffects"] = serde_json::Value::from(3);
    let restored: ExecutionControl = serde_json::from_value(value).expect("deserialize shape");
    assert_eq!(
        restored.validate_restored(),
        Err(ExecutionControlError::CorruptRestoredState)
    );
}

#[test]
fn cross_day_pause_does_not_consume_active_wall_clock_budget() {
    let mut control = control(
        AgentRunId::new(),
        RunBudgetLimits::new(RunBudgetLimitsDraft {
            max_inference_turns: 1,
            max_tool_executions: 0,
            max_tokens: 100,
            max_cost: usd(10),
            max_wall_clock_millis: 200,
            max_preview_renders: 0,
            max_side_effects: 0,
            max_asset_bytes: 0,
            max_concurrent_tools: 0,
        })
        .expect("cross-day budget"),
    );
    let two_days_later = 1_000 + 48 * 60 * 60 * 1_000;
    let usage = control
        .record_inference(
            "cross-day-turn",
            InferenceBudgetCharge {
                input_tokens: 25,
                output_tokens: 25,
                wall_clock_millis: 100,
                cost: usd(1),
            },
            two_days_later,
        )
        .expect("paused calendar time is not active execution time");
    assert_eq!(usage.wall_clock_millis(), 100);
}

#[test]
fn manager_commit_failure_does_not_publish_a_partial_grant() {
    let run_id = AgentRunId::new();
    let store = Arc::new(FaultStore::default());
    let manager = ExecutionControlManager::new(store.clone());
    let initial = control(run_id.clone(), limits(8, 4, 1_000, 100, 1));
    manager.configure(&initial).expect("configure memory store");
    store.fail_commits.store(true, Ordering::SeqCst);
    assert_eq!(
        manager.apply(
            &run_id,
            0,
            ExecutionControlCommand::IssueGrant(grant(
                ApprovalGrantId::new(),
                run_id.clone(),
                42,
                vec!["track:piano".to_owned()],
                4,
                100,
                None,
            )),
        ),
        Err(ExecutionControlManagerError::Store(
            ExecutionControlStoreError::Unavailable("injected commit failure".to_owned())
        ))
    );
    let unchanged = manager.inspect(&run_id).expect("unchanged snapshot");
    assert_eq!(unchanged.revision(), 0);
    assert!(unchanged.control().grants().is_empty());
}

#[derive(Default)]
struct FaultStore {
    snapshot: Mutex<Option<ExecutionControlSnapshot>>,
    fail_commits: AtomicBool,
}

impl ExecutionControlStore for FaultStore {
    fn create_execution_control(
        &self,
        control: &ExecutionControl,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlStoreError> {
        let mut slot = self.snapshot.lock().expect("memory store lock");
        if slot.is_some() {
            return Err(ExecutionControlStoreError::AlreadyExists);
        }
        let snapshot = ExecutionControlSnapshot::new(0, control.clone())
            .map_err(|error| ExecutionControlStoreError::Corrupt(error.to_string()))?;
        *slot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn load_execution_control(
        &self,
        run_id: &AgentRunId,
    ) -> Result<Option<ExecutionControlSnapshot>, ExecutionControlStoreError> {
        Ok(self
            .snapshot
            .lock()
            .expect("memory store lock")
            .as_ref()
            .filter(|snapshot| snapshot.control().run_id() == run_id)
            .cloned())
    }

    fn commit_execution_control(
        &self,
        expected_revision: u64,
        control: &ExecutionControl,
    ) -> Result<ExecutionControlSnapshot, ExecutionControlStoreError> {
        if self.fail_commits.load(Ordering::SeqCst) {
            return Err(ExecutionControlStoreError::Unavailable(
                "injected commit failure".to_owned(),
            ));
        }
        let mut slot = self.snapshot.lock().expect("memory store lock");
        let current = slot.as_ref().ok_or(ExecutionControlStoreError::NotFound)?;
        if current.revision() != expected_revision {
            return Err(ExecutionControlStoreError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision(),
            });
        }
        let next = ExecutionControlSnapshot::new(expected_revision + 1, control.clone())
            .map_err(|error| ExecutionControlStoreError::Corrupt(error.to_string()))?;
        *slot = Some(next.clone());
        Ok(next)
    }
}

fn authorize(
    control: &mut ExecutionControl,
    claim: ToolExecutionClaim,
    now: u64,
) -> Result<autostudio_core::execution_control::ToolBudgetReservation, ExecutionControlError> {
    control.authorize_tool(ExecutionReservationId::new(), claim, resource_limit(), now)
}

fn control(run_id: AgentRunId, configured: RunBudgetLimits) -> ExecutionControl {
    ExecutionControl::new(
        run_id,
        configured,
        limits(64, 64, 1_000_000, 100_000, 8),
        1_000,
    )
    .expect("Execution Control")
}

fn limits(turns: u64, tools: u64, tokens: u64, cost: u64, concurrency: u64) -> RunBudgetLimits {
    RunBudgetLimits::new(RunBudgetLimitsDraft {
        max_inference_turns: turns,
        max_tool_executions: tools,
        max_tokens: tokens,
        max_cost: usd(cost),
        max_wall_clock_millis: 60_000,
        max_preview_renders: 16,
        max_side_effects: 64,
        max_asset_bytes: 1_000_000,
        max_concurrent_tools: concurrency,
    })
    .expect("Run Budget limits")
}

fn grant(
    id: ApprovalGrantId,
    run_id: AgentRunId,
    revision: u64,
    targets: Vec<String>,
    max_effects: u64,
    max_cost: u64,
    expires_at: Option<u64>,
) -> ApprovalGrant {
    ApprovalGrant::issue(
        id,
        ApprovalGrantDraft {
            creator_action_id: "creator-action-1".to_owned(),
            run_id,
            project_id: "project-1".to_owned(),
            project_revision: revision,
            subject: subject(),
            tool_descriptor_fingerprint: digest("tool"),
            targets,
            side_effect_class: SideEffectClass::ProjectMutation,
            max_effects,
            max_cost: Some(usd(max_cost)),
            issued_at_unix_millis: 1_000,
            expires_at_unix_millis: expires_at,
        },
    )
    .expect("Approval Grant")
}

fn claim(
    grant_id: ApprovalGrantId,
    run_id: AgentRunId,
    revision: u64,
    targets: Vec<String>,
    budget_charge: ToolBudgetCharge,
    resources: ToolResourceUsage,
) -> ToolExecutionClaim {
    ToolExecutionClaim::new(ToolExecutionClaimDraft {
        grant_id,
        run_id,
        project_id: "project-1".to_owned(),
        project_revision: revision,
        subject: subject(),
        tool_descriptor_fingerprint: digest("tool"),
        targets,
        side_effect_class: SideEffectClass::ProjectMutation,
        budget_charge,
        resources,
    })
    .expect("Tool claim")
}

fn charge(effects: u64, cost: u64) -> ToolBudgetCharge {
    ToolBudgetCharge {
        side_effects: effects,
        preview_renders: 0,
        asset_bytes: 0,
        wall_clock_millis: 1,
        cost: usd(cost),
    }
}

fn resource_limit() -> ToolResourceLimit {
    ToolResourceLimit::new(ToolResourceLimitDraft {
        max_input_bytes: 2_000,
        max_target_count: 4,
        max_cpu_millis: 2_000,
        max_memory_bytes: 4_096,
        max_output_bytes: 2_000,
        deadline_millis: 5_000,
    })
}

fn resource_usage(
    input_bytes: u64,
    target_count: u64,
    cpu_millis: u64,
    memory_bytes: u64,
    output_bytes: u64,
    deadline_millis: u64,
) -> ToolResourceUsage {
    ToolResourceUsage {
        input_bytes,
        target_count,
        cpu_millis,
        memory_bytes,
        output_bytes,
        deadline_millis,
    }
}

fn subject() -> ApprovalSubject {
    ApprovalSubject::Plan {
        input_hash: digest("plan"),
    }
}

fn usd(minor_units: u64) -> Money {
    Money::new("USD", minor_units).expect("USD amount")
}

fn digest(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}
