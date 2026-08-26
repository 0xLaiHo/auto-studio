use std::sync::Arc;

use autostudio_core::agent::{AgentRunFailureKind, AgentRunStatus};
use autostudio_core::context::{
    ContextEvent, ContextEventEnvelope, ContextEventStore, ContextStoreError,
    InferenceFinishReason, InferenceItemDraft,
};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::{
    AdapterError, AgentPlanner, DeterministicInferenceAdapter, InferenceAdapter, InferenceFuture,
    InferenceProviderDescriptor, InferenceTurnRequest,
};

#[tokio::test]
async fn context_failure_terminates_the_run_without_calling_the_provider() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(
            &temp.path().join("context-failure.autostudio"),
        )
        .expect("project store"),
    );
    let projects = Arc::new(ProjectService::new(store));
    projects.create_project("Context Failure").expect("project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A short cue".to_owned(),
                purpose: None,
                style: vec![],
                mood: vec![],
                instrumentation: vec![],
                target_duration_seconds: Some(30),
                lyrics: None,
                constraints: vec![],
            },
        )
        .expect("brief");
    let contexts = Arc::new(autostudio_provider::context::ContextManager::new(Arc::new(
        FailingContextStore,
    )));
    let error = AgentPlanner::new(
        projects.clone(),
        contexts,
        Arc::new(DeterministicInferenceAdapter),
    )
    .plan(1)
    .await
    .expect_err("Context failure must terminate planning");
    assert!(matches!(
        error,
        autostudio_provider::AgentPlannerError::Context(_)
    ));

    let failed = projects.open_project().expect("failed Project");
    assert_eq!(failed.revision(), 3);
    let run = failed.agent_runs().first().expect("durable Agent Run");
    assert_eq!(run.status(), AgentRunStatus::Failed);
    assert!(run.plan_value().is_none());
    assert_eq!(
        run.failure().expect("failure record").kind(),
        AgentRunFailureKind::HarnessUnavailable
    );
}

#[tokio::test]
async fn provider_rejection_leaves_a_visible_failed_run_and_allows_replanning() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(
            &temp.path().join("planning-failure.autostudio"),
        )
        .expect("project store"),
    );
    let projects = Arc::new(ProjectService::new(store.clone()));
    let contexts = Arc::new(autostudio_provider::context::ContextManager::new(
        store.clone(),
    ));
    projects
        .create_project("Planning Failure")
        .expect("project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A short cue".to_owned(),
                purpose: None,
                style: vec![],
                mood: vec![],
                instrumentation: vec![],
                target_duration_seconds: Some(30),
                lyrics: None,
                constraints: vec![],
            },
        )
        .expect("brief");

    let planner = AgentPlanner::new(
        projects.clone(),
        contexts.clone(),
        Arc::new(RejectingInference {
            projects: projects.clone(),
        }),
    );
    let error = planner
        .plan(1)
        .await
        .expect_err("Provider rejection must fail planning");
    assert!(matches!(
        error,
        autostudio_provider::AgentPlannerError::Adapter(AdapterError::Rejected(_))
    ));

    let failed = projects.open_project().expect("failed Project");
    assert_eq!(failed.revision(), 3);
    let run = failed.agent_runs().first().expect("durable Agent Run");
    assert_eq!(run.status(), AgentRunStatus::Failed);
    assert!(run.plan_value().is_none());
    assert_eq!(
        run.failure().expect("failure record").kind(),
        AgentRunFailureKind::ProviderRejected
    );

    let events = store.context_events(run.id()).expect("context journal");
    assert!(events.iter().any(|event| {
        matches!(
            event.event(),
            ContextEvent::InferenceItemAppended { item }
                if matches!(
                    item.payload(),
                    InferenceItemDraft::Finish {
                        reason: InferenceFinishReason::ProviderRejected,
                        ..
                    }
                )
        )
    }));

    let replanned = AgentPlanner::new(projects, contexts, Arc::new(DeterministicInferenceAdapter))
        .plan(failed.revision())
        .await
        .expect("terminal planning failure permits a new Run");
    assert_eq!(replanned.agent_runs().len(), 2);
    assert_eq!(
        replanned.agent_runs()[1].status(),
        AgentRunStatus::AwaitingApproval
    );
}

struct RejectingInference {
    projects: Arc<ProjectService>,
}

struct FailingContextStore;

impl ContextEventStore for FailingContextStore {
    fn append_context_events(
        &self,
        _run_id: &autostudio_core::agent::AgentRunId,
        _expected_revision: u64,
        _events: &[ContextEvent],
    ) -> Result<u64, ContextStoreError> {
        Err(ContextStoreError::Unavailable(
            "contract context store failure".to_owned(),
        ))
    }

    fn context_events(
        &self,
        _run_id: &autostudio_core::agent::AgentRunId,
    ) -> Result<Vec<ContextEventEnvelope>, ContextStoreError> {
        Ok(Vec::new())
    }
}

impl InferenceAdapter for RejectingInference {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        InferenceProviderDescriptor {
            provider_kind: "rejection-contract".to_owned(),
            model: "rejection-contract-model".to_owned(),
            thinking_level: ThinkingLevel::Off,
            thinking_control: ThinkingControl::Unsupported,
            thinking_budget_tokens: None,
            capability_revision: "rejection-contract/1".to_owned(),
            mapping_revision: "rejection-contract-mapping/1".to_owned(),
            protocol: "contract".to_owned(),
        }
    }

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        let project = self
            .projects
            .open_project()
            .expect("Project before Provider call");
        let run = project.agent_runs().last().expect("visible planning Run");
        assert_eq!(run.status(), AgentRunStatus::Planning);
        assert_eq!(run.id(), request.prepared.manifest().run_id());
        Box::pin(async {
            Err(AdapterError::Rejected(
                "contract rejection before a plan exists".to_owned(),
            ))
        })
    }
}
