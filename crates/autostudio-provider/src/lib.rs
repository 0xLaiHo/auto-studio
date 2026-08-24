//! Provider registry, inference, and media-generation adapters.

pub mod connection;
pub mod constants;
mod error;
pub mod llm;
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
    AgentDecision, AgentPlanDraft, AgentRunFailureDraft, AgentRunFailureKind, AgentRunId,
    AgentRunStatus, CostEstimate, GenerationAttemptDraft, GenerationIntent, GenerationJobDraft,
    InferenceProvenance, InferenceUsage,
};
use autostudio_core::production::{
    CandidateDraft, GeneratedAssetSink, ProvenanceRecord, RightsDeclaration,
};
use autostudio_core::project::{CreativeBrief, Project, ProjectService};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_core::runtime::{CreativeRuntime, CreativeRuntimeError, CreativeRuntimeFuture};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub use error::{
    AdapterError, AgentPlannerError, ConnectionStoreError, GenerationCoordinatorError,
    ProviderConfigError,
};

pub type InferenceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InferenceOutcome, AdapterError>> + Send + 'a>>;

pub trait InferenceAdapter: Send + Sync {
    fn descriptor(&self) -> InferenceProviderDescriptor;
    fn infer(&self, request: InferenceRequest) -> InferenceFuture<'_>;
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceRequest {
    pub brief: CreativeBrief,
    pub context_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceOutcome {
    pub provider: InferenceProviderDescriptor,
    pub visible_summary: String,
    pub decision: AgentDecision,
    pub estimated_cost: CostEstimate,
    pub usage: Usage,
    pub response_id: Option<String>,
}

pub type Usage = InferenceUsage;

pub struct AgentPlanner {
    projects: Arc<ProjectService>,
    inference: Arc<dyn InferenceAdapter>,
}

impl AgentPlanner {
    #[must_use]
    pub fn new(projects: Arc<ProjectService>, inference: Arc<dyn InferenceAdapter>) -> Self {
        Self {
            projects,
            inference,
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
        let snapshot_bytes =
            serde_json::to_vec(&(project.id().as_str(), project.revision(), &brief))
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))?;
        let input_hash = format!("sha256:{:x}", Sha256::digest(snapshot_bytes));
        let outcome = self
            .inference
            .infer(InferenceRequest {
                brief,
                context_revision: expected_revision,
            })
            .await?;
        let descriptor = outcome.provider;
        self.projects
            .plan_agent_run(
                expected_revision,
                AgentPlanDraft {
                    visible_summary: outcome.visible_summary,
                    decision: outcome.decision,
                    estimated_cost: outcome.estimated_cost,
                    usage: outcome.usage,
                    inference: InferenceProvenance {
                        provider_kind: descriptor.provider_kind,
                        model: descriptor.model,
                        thinking_level: descriptor.thinking_level,
                        thinking_control: descriptor.thinking_control,
                        thinking_budget_tokens: descriptor.thinking_budget_tokens,
                        capability_revision: descriptor.capability_revision,
                        mapping_revision: descriptor.mapping_revision,
                        protocol: descriptor.protocol,
                        response_id: outcome.response_id,
                    },
                    input_hash,
                },
            )
            .map_err(Into::into)
    }
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

    fn infer(&self, request: InferenceRequest) -> InferenceFuture<'_> {
        Box::pin(async move {
            Ok(InferenceOutcome {
                provider: self.descriptor(),
                visible_summary: "Generate two contrasting music Candidates for A/B review"
                    .to_owned(),
                decision: AgentDecision::GenerateMusic(GenerationIntent {
                    prompt: request.brief.summary().to_owned(),
                    duration_seconds: request.brief.target_duration_seconds().unwrap_or(60),
                    candidate_count: 2,
                }),
                estimated_cost: CostEstimate::Known {
                    currency: "USD".to_owned(),
                    lower_minor_units: 50,
                    upper_minor_units: 100,
                },
                usage: Usage {
                    input_tokens: Some(42),
                    output_tokens: Some(12),
                    actual_cost_minor_units: None,
                    currency: None,
                },
                response_id: Some("deterministic-response".to_owned()),
            })
        })
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
        let (intent, input_hash) = match run.plan_value().decision() {
            AgentDecision::GenerateMusic(intent) => {
                (intent.clone(), run.plan_value().input_hash().to_owned())
            }
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
        let expected = match run.plan_value().decision() {
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
        AgentPlannerError::Project(error) => CreativeRuntimeError::Project(error),
    }
}

fn runtime_generation_error(error: GenerationCoordinatorError) -> CreativeRuntimeError {
    match error {
        GenerationCoordinatorError::RunNotFound => {
            CreativeRuntimeError::Rejected("Agent Run was not found".to_owned())
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
        AdapterError::Unavailable(_) => AgentRunFailureKind::ProviderUnavailable,
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
        AdapterError::Unavailable(message) => CreativeRuntimeError::Unavailable(message),
    }
}
