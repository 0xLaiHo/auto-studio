use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use autostudio_core::agent::{AgentRunFailureKind, AgentRunStatus, InferenceUsage};
use autostudio_core::context::{CanonicalToolCall, ContextError, InferenceFinishReason};
use autostudio_core::context_surface::ContextPreparationReason;
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::constants::{PLAN_TOOL_NAME, PROJECT_DESCRIBE_TOOL_NAME};
use autostudio_provider::context::ContextManager;
use autostudio_provider::{
    AdapterError, AgentPlanner, AgentPlannerError, InferenceAdapter, InferenceFuture,
    InferenceOutcome, InferenceProviderDescriptor, InferenceTurnRequest,
};

#[tokio::test]
async fn explicit_context_overflow_compacts_once_then_completes_the_run() {
    let fixture = Fixture::new("overflow-recovers", false);
    let project = fixture
        .planner
        .plan(1)
        .await
        .expect("single overflow recovery");

    assert_eq!(fixture.calls.load(Ordering::SeqCst), 5);
    let run = project.agent_runs().last().expect("planned Run");
    assert_eq!(run.status(), AgentRunStatus::AwaitingApproval);
    let projection = fixture
        .contexts
        .inspect_run(run.id())
        .expect("Context journal");
    assert_eq!(projection.checkpoints().len(), 1);
    assert_eq!(
        projection
            .manifests()
            .iter()
            .filter(|manifest| {
                manifest.preparation_reason() == ContextPreparationReason::ProviderOverflowRecovery
            })
            .count(),
        1
    );
    assert_eq!(overflow_finish_count(&projection), 1);
}

#[tokio::test]
async fn a_second_context_overflow_stops_without_an_unbounded_retry_loop() {
    let fixture = Fixture::new("overflow-stops", true);
    let error = fixture
        .planner
        .plan(1)
        .await
        .expect_err("second overflow must stop");

    assert!(matches!(
        error,
        AgentPlannerError::Context(ContextError::OverflowRecoveryExhausted)
    ));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 5);
    let project = fixture.projects.open_project().expect("failed Project");
    let run = project.agent_runs().last().expect("failed Run");
    assert_eq!(run.status(), AgentRunStatus::Failed);
    assert_eq!(
        run.failure().expect("failure").kind(),
        AgentRunFailureKind::ProviderRejected
    );
    let projection = fixture
        .contexts
        .inspect_run(run.id())
        .expect("Context journal");
    assert_eq!(projection.checkpoints().len(), 1);
    assert_eq!(overflow_finish_count(&projection), 2);
}

fn overflow_finish_count(projection: &autostudio_provider::context::ContextProjection) -> usize {
    projection
        .items()
        .iter()
        .filter(|item| {
            matches!(
                item.payload(),
                autostudio_core::context::InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::ContextOverflow,
                    ..
                }
            )
        })
        .count()
}

struct Fixture {
    _temp: tempfile::TempDir,
    projects: Arc<ProjectService>,
    contexts: Arc<ContextManager>,
    planner: AgentPlanner,
    calls: Arc<AtomicUsize>,
}

impl Fixture {
    fn new(name: &str, overflow_twice: bool) -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let store = Arc::new(
            autostudio_storage::SqliteProjectStore::open(
                &temp.path().join(format!("{name}.autostudio")),
            )
            .expect("Project store"),
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
        let contexts = Arc::new(ContextManager::new(store));
        let calls = Arc::new(AtomicUsize::new(0));
        let planner = AgentPlanner::new(
            projects.clone(),
            contexts.clone(),
            Arc::new(OverflowInference {
                calls: calls.clone(),
                overflow_twice,
            }),
        );
        Self {
            _temp: temp,
            projects,
            contexts,
            planner,
            calls,
        }
    }
}

struct OverflowInference {
    calls: Arc<AtomicUsize>,
    overflow_twice: bool,
}

impl InferenceAdapter for OverflowInference {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        descriptor()
    }

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let overflow_twice = self.overflow_twice;
        Box::pin(async move {
            match call {
                0 => Ok(tool_outcome(
                    &request,
                    PROJECT_DESCRIBE_TOOL_NAME,
                    "{}".to_owned(),
                    call,
                )),
                1 => Ok(visible_outcome(
                    &format!("bounded analysis {}", "x".repeat(16_000)),
                    call,
                )),
                2 => Ok(visible_outcome("final preflight before plan", call)),
                3 => Err(AdapterError::ContextOverflow(
                    "context_length_exceeded: maximum context length exceeded".to_owned(),
                )),
                4 if overflow_twice => Err(AdapterError::ContextOverflow(
                    "context_length_exceeded: maximum context length exceeded again".to_owned(),
                )),
                4 => {
                    assert_eq!(
                        request.prepared.manifest().preparation_reason(),
                        ContextPreparationReason::ProviderOverflowRecovery
                    );
                    Ok(tool_outcome(
                        &request,
                        PLAN_TOOL_NAME,
                        serde_json::json!({
                            "visibleSummary": "Generate two warm piano directions",
                            "generationPrompt": "warm acoustic piano cue",
                            "durationSeconds": 30,
                            "candidateCount": 2
                        })
                        .to_string(),
                        call,
                    ))
                }
                _ => panic!("unexpected Provider retry {call}"),
            }
        })
    }
}

fn tool_outcome(
    request: &InferenceTurnRequest,
    name: &str,
    arguments_json: String,
    call: usize,
) -> InferenceOutcome {
    assert!(
        request
            .prepared
            .tools()
            .iter()
            .any(|tool| tool.name == name)
    );
    InferenceOutcome {
        provider: descriptor(),
        visible_text: None,
        tool_calls: vec![CanonicalToolCall {
            call_id: format!("overflow-call-{call}"),
            name: name.to_owned(),
            arguments_json,
        }],
        usage: InferenceUsage::default(),
        response_id: Some(format!("overflow-response-{call}")),
        continuity: None,
    }
}

fn visible_outcome(content: &str, call: usize) -> InferenceOutcome {
    InferenceOutcome {
        provider: descriptor(),
        visible_text: Some(content.to_owned()),
        tool_calls: Vec::new(),
        usage: InferenceUsage::default(),
        response_id: Some(format!("overflow-response-{call}")),
        continuity: None,
    }
}

fn descriptor() -> InferenceProviderDescriptor {
    InferenceProviderDescriptor {
        provider_kind: "overflow-contract".to_owned(),
        model: "overflow-model".to_owned(),
        thinking_level: ThinkingLevel::Off,
        thinking_control: ThinkingControl::Unsupported,
        thinking_budget_tokens: None,
        capability_revision: "overflow-capability/1".to_owned(),
        mapping_revision: "overflow-mapping/1".to_owned(),
        protocol: "contract".to_owned(),
    }
}
