use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use autostudio_core::agent::{AgentRunFailureKind, AgentRunId, AgentRunStatus, InferenceUsage};
use autostudio_core::context::{
    CanonicalToolCall, CanonicalToolDefinition, InferenceFinishReason, InferenceItemDraft,
    InferenceTurnId, ProviderBinding, TokenBudgetPlan,
};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::constants::{
    EMPTY_OBJECT_SCHEMA_JSON, PLAN_SCHEMA_JSON, PLAN_TOOL_DESCRIPTION, PLAN_TOOL_NAME,
    PROJECT_DESCRIBE_TOOL_DESCRIPTION, PROJECT_DESCRIBE_TOOL_NAME,
};
use autostudio_provider::context::{
    CompletedToolResult, ContextManager, PrepareContext, RecordInferenceTurn, RecordToolResults,
    fingerprint_tool_catalog,
};
use autostudio_provider::{
    AgentPlanner, InferenceAdapter, InferenceFuture, InferenceOutcome, InferenceProviderDescriptor,
    InferenceTurnRequest,
};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn pending_local_tool_is_resumed_before_the_provider_is_called_again() {
    let fixture = Fixture::new("pending-tool");
    let tool = describe_tool();
    let prepared = fixture.prepare(tool.clone(), true);
    fixture
        .contexts
        .record_turn(RecordInferenceTurn {
            run_id: fixture.run_id.clone(),
            turn_id: prepared.manifest().turn_id().clone(),
            context_id: prepared.manifest().context_id().clone(),
            expected_journal_revision: prepared.journal_revision(),
            items: vec![
                InferenceItemDraft::ToolRequest {
                    call_id: "describe-call".to_owned(),
                    name: PROJECT_DESCRIBE_TOOL_NAME.to_owned(),
                    arguments_json: "{}".to_owned(),
                    descriptor_fingerprint: tool.descriptor_fingerprint,
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: Some("response-before-crash".to_owned()),
                },
            ],
        })
        .expect("durable pending Tool Request");

    let calls = Arc::new(AtomicUsize::new(0));
    let planner = fixture.planner(calls.clone());
    let project = planner
        .resume(2, &fixture.run_id)
        .await
        .expect("resume pending Tool Request");

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only the next turn calls the Provider"
    );
    assert_eq!(
        project.agent_runs()[0].status(),
        AgentRunStatus::AwaitingApproval
    );
    let projection = fixture
        .contexts
        .inspect_run(&fixture.run_id)
        .expect("durable transcript");
    assert!(projection.pending_tools().is_empty());
    assert!(projection.items().iter().any(|item| matches!(
        item.payload(),
        InferenceItemDraft::ToolResult { name, .. } if name == PROJECT_DESCRIBE_TOOL_NAME
    )));
}

#[tokio::test]
async fn completed_plan_tool_result_commits_without_another_provider_call() {
    let fixture = Fixture::new("completed-plan");
    let tool = plan_tool();
    let prepared = fixture.prepare(tool.clone(), true);
    let arguments = plan_arguments();
    let recorded = fixture
        .contexts
        .record_turn(RecordInferenceTurn {
            run_id: fixture.run_id.clone(),
            turn_id: prepared.manifest().turn_id().clone(),
            context_id: prepared.manifest().context_id().clone(),
            expected_journal_revision: prepared.journal_revision(),
            items: vec![
                InferenceItemDraft::ToolRequest {
                    call_id: "plan-call".to_owned(),
                    name: PLAN_TOOL_NAME.to_owned(),
                    arguments_json: arguments,
                    descriptor_fingerprint: tool.descriptor_fingerprint,
                },
                InferenceItemDraft::Usage {
                    usage: InferenceUsage {
                        input_tokens: Some(12),
                        output_tokens: Some(8),
                        actual_cost_minor_units: None,
                        currency: None,
                    },
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: Some("plan-response".to_owned()),
                },
            ],
        })
        .expect("record plan Tool Request");
    fixture
        .contexts
        .record_tool_results(RecordToolResults {
            run_id: fixture.run_id.clone(),
            expected_journal_revision: recorded.journal_revision,
            results: vec![CompletedToolResult {
                call_id: "plan-call".to_owned(),
                name: PLAN_TOOL_NAME.to_owned(),
                content: "{\"accepted\":true}".to_owned(),
                is_error: false,
                execution_id: Some("execution-plan".to_owned()),
            }],
        })
        .expect("record plan Tool Result");

    let calls = Arc::new(AtomicUsize::new(0));
    let project = fixture
        .planner(calls.clone())
        .resume(2, &fixture.run_id)
        .await
        .expect("commit recovered plan");

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        project.agent_runs()[0].status(),
        AgentRunStatus::AwaitingApproval
    );
    assert_eq!(
        project.agent_runs()[0]
            .plan_value()
            .expect("recovered plan")
            .usage()
            .input_tokens,
        Some(12)
    );
}

#[tokio::test]
async fn prepared_turn_without_output_fails_instead_of_resubmitting() {
    let fixture = Fixture::new("ambiguous-turn");
    fixture.prepare(describe_tool(), true);
    let calls = Arc::new(AtomicUsize::new(0));

    let error = fixture
        .planner(calls.clone())
        .resume(2, &fixture.run_id)
        .await
        .expect_err("ambiguous Provider consumption must not be retried");

    assert!(matches!(
        error,
        autostudio_provider::AgentPlannerError::InterruptedTurn
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let failed = fixture.projects.open_project().expect("failed Project");
    assert_eq!(failed.agent_runs()[0].status(), AgentRunStatus::Failed);
    assert_eq!(
        failed.agent_runs()[0].failure().expect("failure").kind(),
        AgentRunFailureKind::InferenceInterrupted
    );
}

struct Fixture {
    _temp: tempfile::TempDir,
    projects: Arc<ProjectService>,
    contexts: Arc<ContextManager>,
    run_id: AgentRunId,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let package = temp.path().join(format!("{name}.autostudio"));
        let store = Arc::new(
            autostudio_storage::SqliteProjectStore::open(&package).expect("Project store"),
        );
        let projects = Arc::new(ProjectService::new(store.clone()));
        projects.create_project(name).expect("Project");
        projects
            .set_brief(
                0,
                CreativeBriefDraft {
                    summary: "A warm thirty-second piano cue".to_owned(),
                    purpose: None,
                    style: vec!["acoustic".to_owned()],
                    mood: vec!["warm".to_owned()],
                    instrumentation: vec!["piano".to_owned()],
                    target_duration_seconds: Some(30),
                    lyrics: None,
                    constraints: vec!["instrumental".to_owned()],
                },
            )
            .expect("Brief");
        let run_id = AgentRunId::new();
        projects
            .begin_agent_run(1, run_id.clone())
            .expect("Planning Run");
        Self {
            _temp: temp,
            projects,
            contexts: Arc::new(ContextManager::new(store)),
            run_id,
        }
    }

    fn prepare(
        &self,
        tool: CanonicalToolDefinition,
        include_brief: bool,
    ) -> autostudio_core::context::PreparedContext {
        let project = self.projects.open_project().expect("Project");
        let tools = vec![tool];
        self.contexts
            .prepare_turn(PrepareContext {
                run_id: self.run_id.clone(),
                turn_id: InferenceTurnId::new(),
                project_id: project.id().as_str(),
                project_revision: project.revision(),
                instructions: "Planning recovery contract".to_owned(),
                new_user_messages: if include_brief {
                    vec![
                        serde_json::to_string(project.brief().expect("Brief")).expect("Brief JSON"),
                    ]
                } else {
                    Vec::new()
                },
                provider_binding: binding(&tools),
                continuity_reference: None,
                continuity_overhead_tokens: 0,
                tools,
                token_budget: TokenBudgetPlan::unknown(4_096, 1_024),
            })
            .expect("prepared Context")
    }

    fn planner(&self, calls: Arc<AtomicUsize>) -> AgentPlanner {
        AgentPlanner::new(
            self.projects.clone(),
            self.contexts.clone(),
            Arc::new(CountingInference { calls }),
        )
    }
}

struct CountingInference {
    calls: Arc<AtomicUsize>,
}

impl InferenceAdapter for CountingInference {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        descriptor()
    }

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let described = request.prepared.messages().iter().any(|message| {
            matches!(message, autostudio_core::context::CanonicalMessage::Tool {
                name,
                is_error: false,
                ..
            } if name == PROJECT_DESCRIBE_TOOL_NAME)
        });
        let expected = if described {
            PLAN_TOOL_NAME
        } else {
            PROJECT_DESCRIBE_TOOL_NAME
        };
        let tool = request
            .prepared
            .tools()
            .iter()
            .find(|tool| tool.name == expected)
            .expect("expected recovery Tool")
            .clone();
        Box::pin(async move {
            Ok(InferenceOutcome {
                provider: descriptor(),
                visible_text: None,
                tool_calls: vec![CanonicalToolCall {
                    call_id: "next-plan-call".to_owned(),
                    name: tool.name,
                    arguments_json: plan_arguments(),
                }],
                usage: InferenceUsage {
                    input_tokens: Some(20),
                    output_tokens: Some(10),
                    actual_cost_minor_units: None,
                    currency: None,
                },
                response_id: Some("next-response".to_owned()),
                continuity: None,
            })
        })
    }
}

fn binding(tools: &[CanonicalToolDefinition]) -> ProviderBinding {
    let descriptor = descriptor();
    ProviderBinding {
        provider_kind: descriptor.provider_kind,
        model: descriptor.model,
        protocol: descriptor.protocol,
        thinking_level: descriptor.thinking_level,
        thinking_control: descriptor.thinking_control,
        thinking_budget_tokens: descriptor.thinking_budget_tokens,
        capability_revision: descriptor.capability_revision,
        mapping_revision: descriptor.mapping_revision,
        tool_catalog_fingerprint: fingerprint_tool_catalog(tools),
    }
}

fn descriptor() -> InferenceProviderDescriptor {
    InferenceProviderDescriptor {
        provider_kind: "recovery-contract".to_owned(),
        model: "recovery-model".to_owned(),
        thinking_level: ThinkingLevel::Off,
        thinking_control: ThinkingControl::Unsupported,
        thinking_budget_tokens: None,
        capability_revision: "recovery-capability/1".to_owned(),
        mapping_revision: "recovery-mapping/1".to_owned(),
        protocol: "contract".to_owned(),
    }
}

fn describe_tool() -> CanonicalToolDefinition {
    tool(
        PROJECT_DESCRIBE_TOOL_NAME,
        PROJECT_DESCRIBE_TOOL_DESCRIPTION,
        EMPTY_OBJECT_SCHEMA_JSON,
    )
}

fn plan_tool() -> CanonicalToolDefinition {
    tool(PLAN_TOOL_NAME, PLAN_TOOL_DESCRIPTION, PLAN_SCHEMA_JSON)
}

fn tool(name: &str, description: &str, schema: &str) -> CanonicalToolDefinition {
    let bytes = serde_json::to_vec(&(name, description, schema)).expect("descriptor JSON");
    CanonicalToolDefinition::new(
        name,
        description,
        schema,
        format!("sha256:{:x}", Sha256::digest(bytes)),
    )
    .expect("Tool definition")
}

fn plan_arguments() -> String {
    serde_json::json!({
        "visibleSummary": "Generate two warm piano directions",
        "generationPrompt": "warm acoustic piano cue",
        "durationSeconds": 30,
        "candidateCount": 2
    })
    .to_string()
}
