use std::sync::Arc;

use autostudio_core::agent::AgentRunId;
use autostudio_core::execution_control::{
    ApprovalGrant, ApprovalGrantDraft, ApprovalGrantId, ApprovalSubject, ExecutionControl,
    ExecutionControlCommand, ExecutionControlError, ExecutionControlManager,
    ExecutionControlManagerError, ExecutionControlStoreError, ExecutionReservationId,
    InferenceBudgetCharge, Money, RunBudgetDimension, RunBudgetLimits, RunBudgetLimitsDraft,
    SideEffectClass, ToolBudgetCharge, ToolExecutionClaim, ToolExecutionClaimDraft,
    ToolResourceLimit, ToolResourceLimitDraft, ToolResourceUsage,
};

#[test]
#[allow(clippy::too_many_lines)]
fn sqlite_execution_control_is_cas_durable_and_failed_commands_leave_no_revision() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("execution-control.autostudio");
    let run_id = AgentRunId::new();
    let grant_id = ApprovalGrantId::new();
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("SQLite project store"),
    );
    let manager = ExecutionControlManager::new(store.clone());

    let initial = control(run_id.clone());
    let configured = manager
        .configure(&initial)
        .expect("configure execution control");
    assert_eq!(configured.revision(), 0);
    let granted = manager
        .apply(
            &run_id,
            configured.revision(),
            ExecutionControlCommand::IssueGrant(grant(grant_id.clone(), run_id.clone())),
        )
        .expect("persist Grant");
    assert_eq!(granted.revision(), 1);

    let charge = InferenceBudgetCharge {
        input_tokens: 40,
        output_tokens: 10,
        wall_clock_millis: 100,
        cost: usd(5),
    };
    let charged = manager
        .apply(
            &run_id,
            granted.revision(),
            ExecutionControlCommand::RecordInference {
                turn_id: "turn-1".to_owned(),
                charge: charge.clone(),
                recorded_at_unix_millis: 1_100,
            },
        )
        .expect("persist inference charge");
    assert_eq!(charged.revision(), 2);
    let ambiguous_commit_replay = manager
        .apply(
            &run_id,
            granted.revision(),
            ExecutionControlCommand::RecordInference {
                turn_id: "turn-1".to_owned(),
                charge: charge.clone(),
                recorded_at_unix_millis: 9_998,
            },
        )
        .expect("stale retry recognizes the already committed identity");
    assert_eq!(ambiguous_commit_replay.revision(), 2);
    let replayed = manager
        .apply(
            &run_id,
            charged.revision(),
            ExecutionControlCommand::RecordInference {
                turn_id: "turn-1".to_owned(),
                charge,
                recorded_at_unix_millis: 9_999,
            },
        )
        .expect("idempotent replay");
    assert_eq!(replayed.revision(), 2);

    let stale = manager.apply(
        &run_id,
        1,
        ExecutionControlCommand::RecordInference {
            turn_id: "turn-stale".to_owned(),
            charge: InferenceBudgetCharge {
                input_tokens: 1,
                output_tokens: 1,
                wall_clock_millis: 1,
                cost: usd(0),
            },
            recorded_at_unix_millis: 1_200,
        },
    );
    assert_eq!(
        stale,
        Err(ExecutionControlManagerError::Store(
            ExecutionControlStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }
        ))
    );

    let over_budget = manager.apply(
        &run_id,
        2,
        ExecutionControlCommand::RecordInference {
            turn_id: "turn-over-budget".to_owned(),
            charge: InferenceBudgetCharge {
                input_tokens: 1_000,
                output_tokens: 1_000,
                wall_clock_millis: 1,
                cost: usd(0),
            },
            recorded_at_unix_millis: 1_300,
        },
    );
    assert_eq!(
        over_budget,
        Err(ExecutionControlManagerError::Control(
            ExecutionControlError::RunBudgetExceeded {
                dimension: RunBudgetDimension::Tokens,
            }
        ))
    );
    assert_eq!(manager.inspect(&run_id).unwrap().revision(), 2);

    let reservation_id = ExecutionReservationId::new();
    let reserved = manager
        .apply(
            &run_id,
            2,
            ExecutionControlCommand::AuthorizeTool {
                id: reservation_id,
                claim: claim(grant_id, run_id.clone()),
                resource_limit: resource_limit(),
                requested_at_unix_millis: 1_400,
            },
        )
        .expect("persist Tool reservation");
    assert_eq!(reserved.revision(), 3);
    assert_eq!(reserved.control().reservations().len(), 1);

    drop(manager);
    drop(store);
    let reopened_store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("reopened SQLite store"),
    );
    let reopened = ExecutionControlManager::new(reopened_store);
    let restored = reopened
        .inspect(&run_id)
        .expect("restored execution control");
    assert_eq!(restored, reserved);
    restored
        .control()
        .validate_restored()
        .expect("restored control valid");
}

#[test]
fn corrupt_sqlite_execution_control_fails_closed_on_read() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("corrupt-execution-control.autostudio");
    let run_id = AgentRunId::new();
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("SQLite project store"),
    );
    let manager = ExecutionControlManager::new(store.clone());
    let initial = control(run_id.clone());
    manager
        .configure(&initial)
        .expect("configure execution control");
    drop(manager);
    drop(store);

    let database = rusqlite::Connection::open(package.join("project.db")).expect("raw database");
    database
        .execute(
            "UPDATE agent_run_execution_control
             SET control_json = json_set(control_json, '$.formatRevision', 'tampered')
             WHERE run_id = ?1",
            [run_id.as_str()],
        )
        .expect("tamper stored format");
    drop(database);

    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("reopened SQLite store"),
    );
    let manager = ExecutionControlManager::new(store);
    assert!(matches!(
        manager.inspect(&run_id),
        Err(ExecutionControlManagerError::Store(
            ExecutionControlStoreError::Corrupt(_)
        ))
    ));
}

fn control(run_id: AgentRunId) -> ExecutionControl {
    ExecutionControl::new(
        run_id,
        limits(8, 8, 1_000, 1_000, 2),
        limits(64, 64, 100_000, 100_000, 8),
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
        max_preview_renders: 8,
        max_side_effects: 32,
        max_asset_bytes: 1_000_000,
        max_concurrent_tools: concurrency,
    })
    .expect("Run Budget")
}

fn grant(id: ApprovalGrantId, run_id: AgentRunId) -> ApprovalGrant {
    ApprovalGrant::issue(
        id,
        ApprovalGrantDraft {
            creator_action_id: "creator-action-1".to_owned(),
            run_id,
            project_id: "project-1".to_owned(),
            project_revision: 42,
            subject: subject(),
            tool_descriptor_fingerprint: digest("tool"),
            targets: vec!["track:piano".to_owned()],
            side_effect_class: SideEffectClass::ProjectMutation,
            max_effects: 8,
            max_cost: Some(usd(500)),
            issued_at_unix_millis: 1_000,
            expires_at_unix_millis: None,
        },
    )
    .expect("Approval Grant")
}

fn claim(grant_id: ApprovalGrantId, run_id: AgentRunId) -> ToolExecutionClaim {
    ToolExecutionClaim::new(ToolExecutionClaimDraft {
        grant_id,
        run_id,
        project_id: "project-1".to_owned(),
        project_revision: 42,
        subject: subject(),
        tool_descriptor_fingerprint: digest("tool"),
        targets: vec!["track:piano".to_owned()],
        side_effect_class: SideEffectClass::ProjectMutation,
        budget_charge: ToolBudgetCharge {
            side_effects: 2,
            preview_renders: 1,
            asset_bytes: 512,
            wall_clock_millis: 1_000,
            cost: usd(20),
        },
        resources: ToolResourceUsage {
            input_bytes: 128,
            target_count: 1,
            cpu_millis: 100,
            memory_bytes: 1_024,
            output_bytes: 512,
            deadline_millis: 2_000,
        },
    })
    .expect("Tool claim")
}

fn resource_limit() -> ToolResourceLimit {
    ToolResourceLimit::new(ToolResourceLimitDraft {
        max_input_bytes: 1_024,
        max_target_count: 4,
        max_cpu_millis: 2_000,
        max_memory_bytes: 4_096,
        max_output_bytes: 2_000,
        deadline_millis: 5_000,
    })
}

fn subject() -> ApprovalSubject {
    ApprovalSubject::Plan {
        input_hash: digest("plan"),
    }
}

fn usd(value: u64) -> Money {
    Money::new("USD", value).expect("USD")
}

fn digest(value: &str) -> String {
    let digit = if value == "plan" { '1' } else { '2' };
    format!("sha256:{}", digit.to_string().repeat(64))
}
