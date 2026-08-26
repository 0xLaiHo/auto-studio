//! Provider registry, inference, and media-generation adapters.

pub mod connection;
pub mod constants;
pub mod context;
pub mod continuity;
mod error;
pub mod llm;
mod planning_tools;
pub mod stream;
pub mod thinking;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

#[cfg(any(test, debug_assertions))]
use std::collections::HashMap;
#[cfg(any(test, debug_assertions))]
use std::fs;
#[cfg(any(test, debug_assertions))]
use std::path::Path;
#[cfg(any(test, debug_assertions))]
use std::sync::Mutex;

use autostudio_core::agent::{
    AgentDecision, AgentRunFailureDraft, AgentRunFailureKind, AgentRunId, AgentRunStatus,
    GenerationAttemptDraft, GenerationIntent, GenerationJobDraft, InferenceUsage,
};
#[cfg(any(test, debug_assertions))]
use autostudio_core::context::CanonicalMessage;
use autostudio_core::context::{
    ContextId, InferenceFinishReason, InferenceItemDraft, InferenceTurnId, PreparedContext,
    ProviderBinding, TokenBudgetPlan, VisibleMessageRole,
};
use autostudio_core::continuity::ContinuityBinding;
use autostudio_core::production::{
    CandidateDraft, GeneratedAssetSink, ProvenanceRecord, RightsDeclaration,
};
use autostudio_core::project::{CreativeBrief, Project, ProjectService};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_core::runtime::{CreativeRuntime, CreativeRuntimeError, CreativeRuntimeFuture};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use error::{
    AdapterError, AgentPlannerError, ConnectionStoreError, ContinuityVaultError,
    GenerationCoordinatorError, ProviderConfigError,
};

pub type InferenceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InferenceOutcome, AdapterError>> + Send + 'a>>;

pub trait InferenceAdapter: Send + Sync {
    fn descriptor(&self) -> InferenceProviderDescriptor;
    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceProviderDescriptor {
    pub provider_kind: String,
    pub model: String,
    #[serde(rename = "modelEffort")]
    pub thinking_level: ThinkingLevel,
    pub thinking_control: ThinkingControl,
    pub thinking_budget_tokens: Option<u32>,
    pub capability_revision: String,
    pub mapping_revision: String,
    pub protocol: String,
}

#[derive(Clone, Debug)]
pub struct InferenceTurnRequest {
    pub prepared: PreparedContext,
    pub continuity: Option<continuity::ProviderContinuityState>,
}

#[derive(Clone, Debug)]
pub struct InferenceOutcome {
    pub provider: InferenceProviderDescriptor,
    pub visible_text: Option<String>,
    pub tool_calls: Vec<autostudio_core::context::CanonicalToolCall>,
    pub usage: Usage,
    pub response_id: Option<String>,
    pub continuity: Option<continuity::ProviderContinuityState>,
}

pub type Usage = InferenceUsage;

pub struct AgentPlanner {
    projects: Arc<ProjectService>,
    contexts: Arc<context::ContextManager>,
    inference: Arc<dyn InferenceAdapter>,
    continuity: Arc<dyn continuity::ContinuityVault>,
}

struct PreparedPlanningTurn {
    run_id: AgentRunId,
    turn_id: InferenceTurnId,
    context_id: ContextId,
    journal_revision: u64,
    continuity_binding: ContinuityBinding,
    request: InferenceTurnRequest,
}

impl AgentPlanner {
    /// Creates a planner without private continuity for deterministic debug/test fixtures.
    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub fn new(
        projects: Arc<ProjectService>,
        contexts: Arc<context::ContextManager>,
        inference: Arc<dyn InferenceAdapter>,
    ) -> Self {
        Self::with_continuity_vault(
            projects,
            contexts,
            inference,
            Arc::new(continuity::DisabledContinuityVault),
        )
    }

    #[must_use]
    pub fn with_continuity_vault(
        projects: Arc<ProjectService>,
        contexts: Arc<context::ContextManager>,
        inference: Arc<dyn InferenceAdapter>,
        continuity: Arc<dyn continuity::ContinuityVault>,
    ) -> Self {
        Self {
            projects,
            contexts,
            inference,
            continuity,
        }
    }

    /// Builds a Context Snapshot, asks the Agent Model for one typed Decision,
    /// and persists the visible Agent Plan in an approval-waiting state.
    ///
    /// # Errors
    ///
    /// Returns [`AgentPlannerError`] when the Project has no Brief, inference
    /// fails, the revision changes, or the Project transaction fails.
    pub async fn plan(&self, expected_revision: u64) -> Result<Project, AgentPlannerError> {
        let project = self.projects.open_project()?;
        if project.revision() != expected_revision {
            return Err(AgentPlannerError::Project(
                autostudio_core::project::ProjectStoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: project.revision(),
                }
                .into(),
            ));
        }
        let brief = project
            .brief()
            .cloned()
            .ok_or(AgentPlannerError::MissingBrief)?;
        let run_id = AgentRunId::new();
        self.projects
            .begin_agent_run(expected_revision, run_id.clone())?;
        match self.drive(run_id.clone(), &brief).await {
            Ok(project) => Ok(project),
            Err(error) => {
                self.fail_planning_run(&run_id, &error)?;
                Err(error)
            }
        }
    }

    /// Resumes a durable Planning Run using only the Project and transcript.
    ///
    /// # Errors
    ///
    /// Returns [`AgentPlannerError`] for a missing/non-Planning Run, revision
    /// conflict, ambiguous prepared Provider turn, or failed next step.
    pub async fn resume(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
    ) -> Result<Project, AgentPlannerError> {
        let project = self.projects.open_project()?;
        if project.revision() != expected_revision {
            return Err(AgentPlannerError::Project(
                autostudio_core::project::ProjectStoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: project.revision(),
                }
                .into(),
            ));
        }
        let run = project
            .agent_runs()
            .iter()
            .find(|run| run.id() == run_id)
            .ok_or(AgentPlannerError::RunNotFound)?;
        if run.status() != AgentRunStatus::Planning {
            return Err(AgentPlannerError::RunNotPlanning);
        }
        let brief = project
            .brief()
            .cloned()
            .ok_or(AgentPlannerError::MissingBrief)?;
        match self.drive(run_id.clone(), &brief).await {
            Ok(project) => Ok(project),
            Err(error) => {
                self.fail_planning_run(run_id, &error)?;
                Err(error)
            }
        }
    }

    async fn drive(
        &self,
        run_id: AgentRunId,
        brief: &CreativeBrief,
    ) -> Result<Project, AgentPlannerError> {
        for _ in 0..constants::PLANNING_MAX_STEPS {
            let project = self.projects.open_project()?;
            let run = project
                .agent_runs()
                .iter()
                .find(|run| run.id() == &run_id)
                .ok_or(AgentPlannerError::RunNotFound)?;
            if run.status() != AgentRunStatus::Planning {
                return Err(AgentPlannerError::RunNotPlanning);
            }
            let projection = self.contexts.inspect_run(&run_id)?;
            if let Some(plan) = planning_tools::completed_plan(&projection)? {
                // A successful terminal semantic commit must never outlive private
                // Provider state that the Harness failed to delete.
                self.continuity.purge_run(&run_id)?;
                let project = self
                    .projects
                    .record_agent_plan(project.revision(), &run_id, plan)
                    .map_err(AgentPlannerError::from)?;
                return Ok(project);
            }
            if projection.prepared_turn_without_output().is_some() {
                return Err(AgentPlannerError::InterruptedTurn);
            }
            if !projection.pending_tools().is_empty() {
                let results = projection
                    .pending_tools()
                    .iter()
                    .map(|request| planning_tools::execute(&project, &projection, request))
                    .collect();
                self.contexts
                    .record_tool_results(context::RecordToolResults {
                        run_id: run_id.clone(),
                        expected_journal_revision: projection.journal_revision(),
                        results,
                    })?;
                continue;
            }
            if projection.manifests().len() >= usize::from(constants::PLANNING_MAX_TURNS) {
                return Err(AgentPlannerError::TurnLimitExceeded);
            }
            let prepared = self.prepare_planning_turn(
                &project,
                brief,
                run_id.clone(),
                projection.items().is_empty(),
                &projection,
            )?;
            self.infer_planning_turn(prepared).await?;
        }
        Err(AgentPlannerError::TurnLimitExceeded)
    }

    fn prepare_planning_turn(
        &self,
        project: &Project,
        brief: &CreativeBrief,
        run_id: AgentRunId,
        initial_turn: bool,
        projection: &context::ContextProjection,
    ) -> Result<PreparedPlanningTurn, AgentPlannerError> {
        let turn_id = InferenceTurnId::new();
        let descriptor = self.inference.descriptor();
        let tools = planning_tools::catalog(projection)?;
        let provider_binding = ProviderBinding {
            provider_kind: descriptor.provider_kind.clone(),
            model: descriptor.model.clone(),
            protocol: descriptor.protocol.clone(),
            thinking_level: descriptor.thinking_level,
            thinking_control: descriptor.thinking_control,
            thinking_budget_tokens: descriptor.thinking_budget_tokens,
            capability_revision: descriptor.capability_revision.clone(),
            mapping_revision: descriptor.mapping_revision.clone(),
            tool_catalog_fingerprint: context::fingerprint_tool_catalog(&tools),
        };
        let continuity_binding = ContinuityBinding::new(run_id.clone(), provider_binding.clone())
            .map_err(ContinuityVaultError::from)?;
        let loaded_continuity = self.continuity.load(
            &continuity_binding,
            continuity::FileContinuityVault::now_unix_millis()?,
        )?;
        let continuity_reference = loaded_continuity
            .as_ref()
            .map(|loaded| loaded.reference.clone());
        let brief_message = serde_json::to_string(&brief)
            .map_err(|error| AdapterError::InvalidResponse(error.to_string()))?;
        let instructions = format!(
            "{}\nAuthoritative Project binding: id={}, revision={}.",
            constants::PLAN_SYSTEM_PROMPT,
            project.id().as_str(),
            project.revision()
        );
        let prepared = self.contexts.prepare_turn(context::PrepareContext {
            run_id: run_id.clone(),
            turn_id: turn_id.clone(),
            project_id: project.id().as_str(),
            project_revision: project.revision(),
            instructions,
            new_user_messages: if initial_turn {
                vec![brief_message]
            } else {
                Vec::new()
            },
            provider_binding,
            continuity_reference,
            tools,
            token_budget: TokenBudgetPlan::unknown(
                u64::from(constants::PLAN_MAX_OUTPUT_TOKENS),
                constants::CONTEXT_SAFETY_MARGIN_TOKENS,
            ),
        })?;
        let context_id = prepared.manifest().context_id().clone();
        let journal_revision = prepared.journal_revision();
        Ok(PreparedPlanningTurn {
            run_id,
            turn_id,
            context_id,
            journal_revision,
            continuity_binding,
            request: InferenceTurnRequest {
                prepared,
                continuity: loaded_continuity.map(|loaded| loaded.state),
            },
        })
    }

    async fn infer_planning_turn(
        &self,
        turn: PreparedPlanningTurn,
    ) -> Result<(), AgentPlannerError> {
        let outcome = match self.inference.infer(turn.request.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                self.contexts.record_turn(context::RecordInferenceTurn {
                    run_id: turn.run_id,
                    turn_id: turn.turn_id,
                    context_id: turn.context_id,
                    expected_journal_revision: turn.journal_revision,
                    items: vec![InferenceItemDraft::Finish {
                        reason: finish_reason_for_error(&error),
                        detail: Some(error.to_string()),
                    }],
                })?;
                return Err(error.into());
            }
        };
        if !descriptor_matches_binding(
            &outcome.provider,
            turn.request.prepared.manifest().provider_binding(),
        ) {
            let error = AdapterError::InvalidResponse(
                "Provider output descriptor changed after Context preparation".to_owned(),
            );
            self.contexts.record_turn(context::RecordInferenceTurn {
                run_id: turn.run_id,
                turn_id: turn.turn_id,
                context_id: turn.context_id,
                expected_journal_revision: turn.journal_revision,
                items: vec![InferenceItemDraft::Finish {
                    reason: finish_reason_for_error(&error),
                    detail: Some(error.to_string()),
                }],
            })?;
            return Err(error.into());
        }
        let mut items = Vec::new();
        if let Some(content) = &outcome.visible_text {
            items.push(InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::Assistant,
                content: content.clone(),
            });
        }
        for call in &outcome.tool_calls {
            let Some(descriptor_fingerprint) = turn
                .request
                .prepared
                .tools()
                .iter()
                .find(|tool| tool.name == call.name)
                .map(|tool| tool.descriptor_fingerprint.clone())
            else {
                let error = AdapterError::InvalidResponse(format!(
                    "Provider requested unavailable Tool '{}'",
                    call.name
                ));
                self.contexts.record_turn(context::RecordInferenceTurn {
                    run_id: turn.run_id,
                    turn_id: turn.turn_id,
                    context_id: turn.context_id,
                    expected_journal_revision: turn.journal_revision,
                    items: vec![InferenceItemDraft::Finish {
                        reason: finish_reason_for_error(&error),
                        detail: Some(error.to_string()),
                    }],
                })?;
                return Err(error.into());
            };
            items.push(InferenceItemDraft::ToolRequest {
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                arguments_json: call.arguments_json.clone(),
                descriptor_fingerprint,
            });
        }
        items.push(InferenceItemDraft::Usage {
            usage: outcome.usage,
        });
        items.push(InferenceItemDraft::Finish {
            reason: InferenceFinishReason::Completed,
            detail: outcome.response_id,
        });
        self.contexts.record_turn(context::RecordInferenceTurn {
            run_id: turn.run_id.clone(),
            turn_id: turn.turn_id.clone(),
            context_id: turn.context_id,
            expected_journal_revision: turn.journal_revision,
            items,
        })?;
        if let Some(state) = &outcome.continuity {
            self.continuity.store(
                &turn.continuity_binding,
                &turn.turn_id,
                state,
                continuity::FileContinuityVault::now_unix_millis()?,
            )?;
        }
        Ok(())
    }

    fn fail_planning_run(
        &self,
        run_id: &AgentRunId,
        error: &AgentPlannerError,
    ) -> Result<(), AgentPlannerError> {
        let project = self.projects.open_project()?;
        if project
            .agent_runs()
            .iter()
            .find(|run| run.id() == run_id)
            .is_some_and(|run| run.status() == AgentRunStatus::Planning)
        {
            self.projects
                .fail_agent_run(project.revision(), run_id, planning_failure(error))?;
        }
        self.continuity.purge_run(run_id)?;
        Ok(())
    }
}

fn descriptor_matches_binding(
    descriptor: &InferenceProviderDescriptor,
    binding: &ProviderBinding,
) -> bool {
    descriptor.provider_kind == binding.provider_kind
        && descriptor.model == binding.model
        && descriptor.protocol == binding.protocol
        && descriptor.thinking_level == binding.thinking_level
        && descriptor.thinking_control == binding.thinking_control
        && descriptor.thinking_budget_tokens == binding.thinking_budget_tokens
        && descriptor.capability_revision == binding.capability_revision
        && descriptor.mapping_revision == binding.mapping_revision
}

/// Deterministic planning fixture. This item is excluded from release builds
/// and must never be registered by a production composition root.
#[cfg(any(test, debug_assertions))]
#[doc(hidden)]
pub struct DeterministicInferenceAdapter;

#[cfg(any(test, debug_assertions))]
impl InferenceAdapter for DeterministicInferenceAdapter {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        InferenceProviderDescriptor {
            provider_kind: "test-fixture".to_owned(),
            model: "deterministic-plan-v1".to_owned(),
            thinking_level: ThinkingLevel::High,
            thinking_control: ThinkingControl::Effort,
            thinking_budget_tokens: None,
            capability_revision: "test-capability/1".to_owned(),
            mapping_revision: "test-mapping/1".to_owned(),
            protocol: "in-memory".to_owned(),
        }
    }

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        Box::pin(async move {
            let brief = request
                .prepared
                .messages()
                .iter()
                .rev()
                .find_map(|message| match message {
                    CanonicalMessage::User { content } => {
                        serde_json::from_str::<CreativeBrief>(content).ok()
                    }
                    CanonicalMessage::Assistant { .. } | CanonicalMessage::Tool { .. } => None,
                })
                .ok_or_else(|| {
                    AdapterError::InvalidResponse(
                        "deterministic fixture requires a Creative Brief message".to_owned(),
                    )
                })?;
            let described = request.prepared.messages().iter().any(|message| {
                matches!(message, CanonicalMessage::Tool { name, is_error: false, .. }
                    if name == constants::PROJECT_DESCRIBE_TOOL_NAME)
            });
            let expected_tool = if described {
                constants::PLAN_TOOL_NAME
            } else {
                constants::PROJECT_DESCRIBE_TOOL_NAME
            };
            let tool = request
                .prepared
                .tools()
                .iter()
                .find(|tool| tool.name == expected_tool)
                .ok_or_else(|| {
                    AdapterError::InvalidResponse("fixture requires the planning Tool".to_owned())
                })?;
            let arguments_json = if tool.name == constants::PROJECT_DESCRIBE_TOOL_NAME {
                "{}".to_owned()
            } else {
                serde_json::to_string(&serde_json::json!({
                    "visibleSummary": "Generate two contrasting music Candidates for A/B review",
                    "generationPrompt": brief.summary(),
                    "durationSeconds": brief.target_duration_seconds().unwrap_or(60),
                    "candidateCount": 2
                }))
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))?
            };
            Ok(InferenceOutcome {
                provider: self.descriptor(),
                visible_text: None,
                tool_calls: vec![autostudio_core::context::CanonicalToolCall {
                    call_id: Uuid::new_v4().to_string(),
                    name: tool.name.clone(),
                    arguments_json,
                }],
                usage: Usage {
                    input_tokens: Some(42),
                    output_tokens: Some(12),
                    actual_cost_minor_units: None,
                    currency: None,
                },
                response_id: Some("deterministic-response".to_owned()),
                continuity: None,
            })
        })
    }
}

fn finish_reason_for_error(error: &AdapterError) -> InferenceFinishReason {
    match error {
        AdapterError::Rejected(_) => InferenceFinishReason::ProviderRejected,
        AdapterError::UnknownOutcome(_) => InferenceFinishReason::UnknownConsumption,
        AdapterError::InvalidResponse(_) | AdapterError::ContinuityUnavailable(_) => {
            InferenceFinishReason::InvalidResponse
        }
        AdapterError::Unavailable(_) => InferenceFinishReason::ProviderUnavailable,
    }
}

fn planning_failure(error: &AgentPlannerError) -> AgentRunFailureDraft {
    let kind = match error {
        AgentPlannerError::Adapter(AdapterError::Rejected(_)) => {
            AgentRunFailureKind::ProviderRejected
        }
        AgentPlannerError::Adapter(AdapterError::InvalidResponse(_)) => {
            AgentRunFailureKind::InvalidProviderResponse
        }
        AgentPlannerError::Adapter(
            AdapterError::Unavailable(_)
            | AdapterError::UnknownOutcome(_)
            | AdapterError::ContinuityUnavailable(_),
        ) => AgentRunFailureKind::ProviderUnavailable,
        AgentPlannerError::InterruptedTurn => AgentRunFailureKind::InferenceInterrupted,
        AgentPlannerError::Context(_)
        | AgentPlannerError::Continuity(_)
        | AgentPlannerError::Project(_)
        | AgentPlannerError::MissingBrief
        | AgentPlannerError::RunNotFound
        | AgentPlannerError::RunNotPlanning
        | AgentPlannerError::TurnLimitExceeded
        | AgentPlannerError::PlanningTool(_) => AgentRunFailureKind::HarnessUnavailable,
    };
    AgentRunFailureDraft {
        kind,
        message: error.to_string(),
    }
}

pub type GenerationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send + 'a>>;

pub trait GenerationAdapter: Send + Sync {
    fn provider_kind(&self) -> &str;
    fn model(&self) -> &str;
    fn submit(&self, request: GenerationRequest) -> GenerationFuture<'_, GenerationSubmission>;
    fn observe(&self, external_job_id: String) -> GenerationFuture<'_, GenerationObservation>;
    fn reconcile(&self, attempt_id: String) -> GenerationFuture<'_, GenerationReconciliation>;
}

#[derive(Clone, Debug)]
pub struct GenerationRequest {
    pub attempt_id: String,
    pub input_hash: String,
    pub intent: GenerationIntent,
}

#[derive(Clone, Debug)]
pub struct GenerationSubmission {
    pub attempt_id: String,
    pub external_job_id: String,
}

#[derive(Clone, Debug)]
pub enum GenerationObservation {
    Pending,
    Succeeded { artifacts: Vec<GeneratedArtifact> },
}

#[derive(Clone, Debug)]
pub enum GenerationReconciliation {
    NotFound,
    Accepted {
        submission: GenerationSubmission,
    },
    Succeeded {
        submission: GenerationSubmission,
        artifacts: Vec<GeneratedArtifact>,
    },
}

#[derive(Clone, Debug)]
pub struct GeneratedArtifact {
    pub label: String,
    pub staging_path: PathBuf,
    pub credits: Vec<String>,
}

pub struct GenerationCoordinator {
    projects: Arc<ProjectService>,
    generation: Arc<dyn GenerationAdapter>,
    assets: Arc<dyn GeneratedAssetSink>,
}

impl GenerationCoordinator {
    #[must_use]
    pub fn new(
        projects: Arc<ProjectService>,
        generation: Arc<dyn GenerationAdapter>,
        assets: Arc<dyn GeneratedAssetSink>,
    ) -> Self {
        Self {
            projects,
            generation,
            assets,
        }
    }

    /// Executes one approved Generation Decision through durable Attempt, Job,
    /// local Asset commit, and Candidate commit states.
    ///
    /// # Errors
    ///
    /// Returns [`GenerationCoordinatorError`] while leaving the last durable state
    /// available for reconciliation or resume.
    pub async fn execute_approved(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
    ) -> Result<Project, GenerationCoordinatorError> {
        let project = self.projects.open_project()?;
        if project.revision() != expected_revision {
            return Err(GenerationCoordinatorError::Project(
                autostudio_core::project::ProjectStoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: project.revision(),
                }
                .into(),
            ));
        }
        let run = project
            .agent_runs()
            .iter()
            .find(|run| run.id() == run_id)
            .ok_or(GenerationCoordinatorError::RunNotFound)?;
        if run.status() != AgentRunStatus::ReadyToSubmit {
            return Err(GenerationCoordinatorError::RunNotApproved);
        }
        let plan = run
            .plan_value()
            .ok_or(GenerationCoordinatorError::MissingPlan)?;
        let (intent, input_hash) = match plan.decision() {
            AgentDecision::GenerateMusic(intent) => (intent.clone(), plan.input_hash().to_owned()),
        };
        let attempt_id = Uuid::new_v4().to_string();
        let prepared = self.projects.prepare_generation(
            expected_revision,
            run_id,
            GenerationAttemptDraft {
                attempt_id: attempt_id.clone(),
                provider_kind: self.generation.provider_kind().to_owned(),
                model: self.generation.model().to_owned(),
                request_hash: input_hash.clone(),
            },
        )?;

        let submission = match self
            .generation
            .submit(GenerationRequest {
                attempt_id: attempt_id.clone(),
                input_hash: input_hash.clone(),
                intent,
            })
            .await
        {
            Ok(submission) => submission,
            Err(error @ AdapterError::UnknownOutcome(_)) => {
                self.projects
                    .mark_generation_unknown(prepared.revision(), run_id)?;
                return Err(error.into());
            }
            Err(error) => {
                self.projects.mark_generation_failed(
                    prepared.revision(),
                    run_id,
                    adapter_failure(&error),
                )?;
                return Err(error.into());
            }
        };
        let submitted = self.projects.record_generation_submitted(
            prepared.revision(),
            run_id,
            GenerationJobDraft {
                attempt_id: submission.attempt_id,
                external_job_id: submission.external_job_id.clone(),
                provider_kind: self.generation.provider_kind().to_owned(),
                model: self.generation.model().to_owned(),
                request_hash: input_hash.clone(),
            },
        )?;

        match self
            .generation
            .observe(submission.external_job_id.clone())
            .await?
        {
            GenerationObservation::Pending => Ok(submitted),
            GenerationObservation::Succeeded { artifacts } => self.commit_artifacts(
                submitted.revision(),
                run_id,
                &submission.external_job_id,
                &input_hash,
                artifacts,
            ),
        }
    }

    /// Polls one submitted Generation Job and commits results when ready.
    ///
    /// # Errors
    ///
    /// Returns [`GenerationCoordinatorError`] when the Run is not submitted, its
    /// adapter changed, Provider polling fails, or result commit fails.
    pub async fn resume_submitted(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
    ) -> Result<Project, GenerationCoordinatorError> {
        let project = self.projects.open_project()?;
        if project.revision() != expected_revision {
            return Err(GenerationCoordinatorError::Project(
                autostudio_core::project::ProjectStoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: project.revision(),
                }
                .into(),
            ));
        }
        let run = project
            .agent_runs()
            .iter()
            .find(|run| run.id() == run_id)
            .ok_or(GenerationCoordinatorError::RunNotFound)?;
        if run.status() != AgentRunStatus::Submitted {
            return Err(GenerationCoordinatorError::RunNotSubmitted);
        }
        let job = run
            .generation_job()
            .ok_or(GenerationCoordinatorError::MissingJob)?;
        if job.provider_kind() != self.generation.provider_kind()
            || job.model() != self.generation.model()
        {
            return Err(GenerationCoordinatorError::WrongAdapter);
        }
        match self
            .generation
            .observe(job.external_job_id().to_owned())
            .await?
        {
            GenerationObservation::Pending => Ok(project),
            GenerationObservation::Succeeded { artifacts } => self.commit_artifacts(
                expected_revision,
                run_id,
                job.external_job_id(),
                job.request_hash(),
                artifacts,
            ),
        }
    }

    /// Reconciles one Unknown Outcome before permitting any further submit.
    ///
    /// # Errors
    ///
    /// Returns [`GenerationCoordinatorError`] when the Run is not unknown, the
    /// configured adapter does not own its Attempt, the Provider cannot reconcile,
    /// or durable recovery fails.
    pub async fn reconcile_unknown(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
    ) -> Result<Project, GenerationCoordinatorError> {
        let project = self.projects.open_project()?;
        if project.revision() != expected_revision {
            return Err(GenerationCoordinatorError::Project(
                autostudio_core::project::ProjectStoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: project.revision(),
                }
                .into(),
            ));
        }
        let run = project
            .agent_runs()
            .iter()
            .find(|run| run.id() == run_id)
            .ok_or(GenerationCoordinatorError::RunNotFound)?;
        if run.status() != AgentRunStatus::UnknownOutcome {
            return Err(GenerationCoordinatorError::RunNotUnknown);
        }
        let attempt = run
            .generation_attempt()
            .ok_or(GenerationCoordinatorError::MissingAttempt)?;
        if attempt.provider_kind() != self.generation.provider_kind()
            || attempt.model() != self.generation.model()
        {
            return Err(GenerationCoordinatorError::WrongAdapter);
        }
        let input_hash = attempt.request_hash().to_owned();
        let attempt_id = attempt.attempt_id().to_owned();
        match self.generation.reconcile(attempt_id.clone()).await? {
            GenerationReconciliation::NotFound => self
                .projects
                .reconcile_generation_not_found(expected_revision, run_id)
                .map_err(Into::into),
            GenerationReconciliation::Accepted { submission } => self.record_reconciled_submission(
                expected_revision,
                run_id,
                attempt_id,
                input_hash,
                submission,
            ),
            GenerationReconciliation::Succeeded {
                submission,
                artifacts,
            } => {
                let submitted = self.record_reconciled_submission(
                    expected_revision,
                    run_id,
                    attempt_id,
                    input_hash.clone(),
                    submission.clone(),
                )?;
                self.commit_artifacts(
                    submitted.revision(),
                    run_id,
                    &submission.external_job_id,
                    &input_hash,
                    artifacts,
                )
            }
        }
    }

    fn record_reconciled_submission(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        attempt_id: String,
        input_hash: String,
        submission: GenerationSubmission,
    ) -> Result<Project, GenerationCoordinatorError> {
        if submission.attempt_id != attempt_id {
            return Err(GenerationCoordinatorError::MismatchedReconciliation);
        }
        self.projects
            .record_reconciled_submission(
                expected_revision,
                run_id,
                GenerationJobDraft {
                    attempt_id,
                    external_job_id: submission.external_job_id,
                    provider_kind: self.generation.provider_kind().to_owned(),
                    model: self.generation.model().to_owned(),
                    request_hash: input_hash,
                },
            )
            .map_err(Into::into)
    }

    fn commit_artifacts(
        &self,
        expected_revision: u64,
        run_id: &AgentRunId,
        external_job_id: &str,
        input_hash: &str,
        artifacts: Vec<GeneratedArtifact>,
    ) -> Result<Project, GenerationCoordinatorError> {
        let project = self.projects.open_project()?;
        if project.revision() != expected_revision {
            return Err(GenerationCoordinatorError::Project(
                autostudio_core::project::ProjectStoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: project.revision(),
                }
                .into(),
            ));
        }
        let run = project
            .agent_runs()
            .iter()
            .find(|run| run.id() == run_id)
            .ok_or(GenerationCoordinatorError::RunNotFound)?;
        let plan = run
            .plan_value()
            .ok_or(GenerationCoordinatorError::MissingPlan)?;
        let expected = match plan.decision() {
            AgentDecision::GenerateMusic(intent) => usize::from(intent.candidate_count),
        };
        let actual = artifacts.len();
        if actual != expected {
            self.projects.mark_generation_failed(
                expected_revision,
                run_id,
                AgentRunFailureDraft {
                    kind: AgentRunFailureKind::InvalidProviderResponse,
                    message: format!(
                        "Provider returned {actual} Candidates for a Plan requiring {expected}"
                    ),
                },
            )?;
            return Err(GenerationCoordinatorError::CandidateCountMismatch { expected, actual });
        }
        let mut candidates = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            let asset = self
                .assets
                .commit_audio(
                    &artifact.staging_path,
                    ProvenanceRecord {
                        provider_kind: self.generation.provider_kind().to_owned(),
                        model: self.generation.model().to_owned(),
                        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
                        external_job_id: Some(external_job_id.to_owned()),
                        input_hash: input_hash.to_owned(),
                        rights: RightsDeclaration::CreatorOwned,
                        credits: artifact.credits,
                    },
                )
                .map_err(GenerationCoordinatorError::AssetCommit)?;
            candidates.push(CandidateDraft {
                label: artifact.label,
                asset,
                note: None,
            });
        }
        self.projects
            .commit_candidates(expected_revision, run_id, candidates)
            .map_err(Into::into)
    }
}

/// Deterministic media fixture. This item is excluded from release builds and
/// must never be registered by a production composition root.
#[cfg(any(test, debug_assertions))]
#[doc(hidden)]
pub struct DeterministicGenerationAdapter {
    staging_root: PathBuf,
    jobs: Mutex<HashMap<String, Vec<GeneratedArtifact>>>,
    attempts: Mutex<HashMap<String, String>>,
    provider_kind: &'static str,
    model: &'static str,
}

#[cfg(any(test, debug_assertions))]
impl DeterministicGenerationAdapter {
    /// Creates the deterministic CI/development Music Provider.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] when its private staging directory cannot be created.
    pub fn new(staging_root: &Path) -> Result<Self, std::io::Error> {
        fs::create_dir_all(staging_root)?;
        Ok(Self {
            staging_root: staging_root.canonicalize()?,
            jobs: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            provider_kind: "test-fixture-music",
            model: "deterministic-v1",
        })
    }
}

#[cfg(any(test, debug_assertions))]
impl GenerationAdapter for DeterministicGenerationAdapter {
    fn provider_kind(&self) -> &str {
        self.provider_kind
    }

    fn model(&self) -> &str {
        self.model
    }

    fn submit(&self, request: GenerationRequest) -> GenerationFuture<'_, GenerationSubmission> {
        Box::pin(async move {
            let external_job_id = Uuid::new_v4().to_string();
            let mut artifacts = Vec::new();
            for index in 0..request.intent.candidate_count {
                let path = self
                    .staging_root
                    .join(format!("{external_job_id}-{index}.wav"));
                write_deterministic_wav(
                    &path,
                    request.intent.duration_seconds,
                    220 + u32::from(index) * 110,
                )
                .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
                artifacts.push(GeneratedArtifact {
                    label: format!("Direction {}", char::from(b'A' + index)),
                    staging_path: path,
                    credits: Vec::new(),
                });
            }
            self.jobs
                .lock()
                .map_err(|_| AdapterError::Unavailable("fixture job ledger poisoned".to_owned()))?
                .insert(external_job_id.clone(), artifacts);
            self.attempts
                .lock()
                .map_err(|_| {
                    AdapterError::Unavailable("fixture attempt ledger poisoned".to_owned())
                })?
                .insert(request.attempt_id.clone(), external_job_id.clone());
            Ok(GenerationSubmission {
                attempt_id: request.attempt_id.clone(),
                external_job_id,
            })
        })
    }

    fn observe(&self, external_job_id: String) -> GenerationFuture<'_, GenerationObservation> {
        Box::pin(async move {
            let artifacts = self
                .jobs
                .lock()
                .map_err(|_| AdapterError::Unavailable("fixture job ledger poisoned".to_owned()))?
                .get(&external_job_id)
                .cloned()
                .ok_or_else(|| AdapterError::Rejected("Generation Job not found".to_owned()))?;
            Ok(GenerationObservation::Succeeded { artifacts })
        })
    }

    fn reconcile(&self, attempt_id: String) -> GenerationFuture<'_, GenerationReconciliation> {
        Box::pin(async move {
            let Some(external_job_id) = self
                .attempts
                .lock()
                .map_err(|_| {
                    AdapterError::Unavailable("fixture attempt ledger poisoned".to_owned())
                })?
                .get(&attempt_id)
                .cloned()
            else {
                return Ok(GenerationReconciliation::NotFound);
            };
            let artifacts = self
                .jobs
                .lock()
                .map_err(|_| AdapterError::Unavailable("fixture job ledger poisoned".to_owned()))?
                .get(&external_job_id)
                .cloned()
                .ok_or_else(|| {
                    AdapterError::Unavailable("fixture job artifact missing".to_owned())
                })?;
            Ok(GenerationReconciliation::Succeeded {
                submission: GenerationSubmission {
                    attempt_id,
                    external_job_id,
                },
                artifacts,
            })
        })
    }
}

#[cfg(any(test, debug_assertions))]
fn write_deterministic_wav(
    path: &Path,
    duration_seconds: u32,
    frequency_hz: u32,
) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let frames = u64::from(duration_seconds) * u64::from(spec.sample_rate);
    let period = (spec.sample_rate / frequency_hz).max(2);
    for frame in 0..frames {
        let sample = if (frame % u64::from(period)) < u64::from(period / 2) {
            90_000
        } else {
            -90_000
        };
        writer.write_sample(sample)?;
        writer.write_sample(sample)?;
    }
    writer.finalize()
}

pub struct LocalCreativeRuntime {
    planner: AgentPlanner,
    generation: Option<GenerationCoordinator>,
}

impl LocalCreativeRuntime {
    #[must_use]
    pub const fn new(planner: AgentPlanner, generation: GenerationCoordinator) -> Self {
        Self {
            planner,
            generation: Some(generation),
        }
    }

    /// Creates an Agent runtime with real LLM planning while generation remains
    /// unavailable until a real Music Provider is configured.
    #[must_use]
    pub const fn planning_only(planner: AgentPlanner) -> Self {
        Self {
            planner,
            generation: None,
        }
    }
}

impl CreativeRuntime for LocalCreativeRuntime {
    fn plan(&self, expected_revision: u64) -> CreativeRuntimeFuture<'_> {
        Box::pin(async move {
            self.planner
                .plan(expected_revision)
                .await
                .map_err(runtime_planner_error)
        })
    }

    fn resume_planning(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_> {
        Box::pin(async move {
            self.planner
                .resume(expected_revision, &run_id)
                .await
                .map_err(runtime_planner_error)
        })
    }

    fn execute_approved(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_> {
        Box::pin(async move {
            let generation = self.generation.as_ref().ok_or_else(|| {
                CreativeRuntimeError::Unavailable(
                    "no real Music Provider is configured; deterministic fixtures are test-only"
                        .to_owned(),
                )
            })?;
            generation
                .execute_approved(expected_revision, &run_id)
                .await
                .map_err(runtime_generation_error)
        })
    }

    fn reconcile_unknown(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_> {
        Box::pin(async move {
            let generation = self.generation.as_ref().ok_or_else(|| {
                CreativeRuntimeError::Unavailable(
                    "no real Music Provider is configured for reconciliation".to_owned(),
                )
            })?;
            generation
                .reconcile_unknown(expected_revision, &run_id)
                .await
                .map_err(runtime_generation_error)
        })
    }

    fn resume_submitted(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_> {
        Box::pin(async move {
            let generation = self.generation.as_ref().ok_or_else(|| {
                CreativeRuntimeError::Unavailable(
                    "no real Music Provider is configured for this submitted Job".to_owned(),
                )
            })?;
            generation
                .resume_submitted(expected_revision, &run_id)
                .await
                .map_err(runtime_generation_error)
        })
    }
}

fn runtime_planner_error(error: AgentPlannerError) -> CreativeRuntimeError {
    match error {
        AgentPlannerError::MissingBrief => {
            CreativeRuntimeError::Rejected("Creative Brief is required".to_owned())
        }
        AgentPlannerError::Adapter(error) => runtime_adapter_error(error),
        AgentPlannerError::Context(error) => CreativeRuntimeError::Unavailable(error.to_string()),
        AgentPlannerError::Continuity(error) => {
            CreativeRuntimeError::Unavailable(error.to_string())
        }
        AgentPlannerError::Project(error) => CreativeRuntimeError::Project(error),
        AgentPlannerError::RunNotFound | AgentPlannerError::RunNotPlanning => {
            CreativeRuntimeError::Rejected(error.to_string())
        }
        AgentPlannerError::TurnLimitExceeded
        | AgentPlannerError::InterruptedTurn
        | AgentPlannerError::PlanningTool(_) => {
            CreativeRuntimeError::Unavailable(error.to_string())
        }
    }
}

fn runtime_generation_error(error: GenerationCoordinatorError) -> CreativeRuntimeError {
    match error {
        GenerationCoordinatorError::RunNotFound => {
            CreativeRuntimeError::Rejected("Agent Run was not found".to_owned())
        }
        GenerationCoordinatorError::MissingPlan => {
            CreativeRuntimeError::Unavailable("Agent Plan is missing".to_owned())
        }
        GenerationCoordinatorError::RunNotApproved => {
            CreativeRuntimeError::Rejected("Agent Run requires Approval".to_owned())
        }
        GenerationCoordinatorError::RunNotUnknown => {
            CreativeRuntimeError::Rejected("Agent Run does not require reconciliation".to_owned())
        }
        GenerationCoordinatorError::RunNotSubmitted => {
            CreativeRuntimeError::Rejected("Agent Run has no submitted Job".to_owned())
        }
        GenerationCoordinatorError::MissingAttempt => {
            CreativeRuntimeError::Unavailable("Generation Attempt is missing".to_owned())
        }
        GenerationCoordinatorError::MissingJob => {
            CreativeRuntimeError::Unavailable("Generation Job is missing".to_owned())
        }
        GenerationCoordinatorError::WrongAdapter => {
            CreativeRuntimeError::Unavailable("Music Provider adapter changed".to_owned())
        }
        GenerationCoordinatorError::MismatchedReconciliation => {
            CreativeRuntimeError::Unavailable("Provider reconciliation was invalid".to_owned())
        }
        GenerationCoordinatorError::AssetCommit(error) => CreativeRuntimeError::Unavailable(error),
        GenerationCoordinatorError::CandidateCountMismatch { expected, actual } => {
            CreativeRuntimeError::Rejected(format!(
                "Provider returned {actual} Candidates; the approved Plan requires {expected}"
            ))
        }
        GenerationCoordinatorError::Adapter(error) => runtime_adapter_error(error),
        GenerationCoordinatorError::Project(error) => CreativeRuntimeError::Project(error),
    }
}

fn adapter_failure(error: &AdapterError) -> AgentRunFailureDraft {
    let kind = match error {
        AdapterError::Rejected(_) => AgentRunFailureKind::ProviderRejected,
        AdapterError::UnknownOutcome(_) | AdapterError::InvalidResponse(_) => {
            AgentRunFailureKind::InvalidProviderResponse
        }
        AdapterError::Unavailable(_) | AdapterError::ContinuityUnavailable(_) => {
            AgentRunFailureKind::ProviderUnavailable
        }
    };
    AgentRunFailureDraft {
        kind,
        message: error.to_string(),
    }
}

fn runtime_adapter_error(error: AdapterError) -> CreativeRuntimeError {
    match error {
        AdapterError::UnknownOutcome(message) => CreativeRuntimeError::UnknownOutcome(message),
        AdapterError::Rejected(message) => CreativeRuntimeError::Rejected(message),
        AdapterError::InvalidResponse(error) => CreativeRuntimeError::Unavailable(error),
        AdapterError::Unavailable(message) | AdapterError::ContinuityUnavailable(message) => {
            CreativeRuntimeError::Unavailable(message)
        }
    }
}
