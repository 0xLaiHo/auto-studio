use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use autostudio_core::agent::{AgentRunId, AgentRunStatus, CostApproval};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_media::ProjectMedia;
use autostudio_provider::{
    AdapterError, AgentPlanner, DeterministicInferenceAdapter, GeneratedArtifact,
    GenerationAdapter, GenerationCoordinator, GenerationCoordinatorError, GenerationFuture,
    GenerationObservation, GenerationReconciliation, GenerationRequest, GenerationSubmission,
};

#[tokio::test]
async fn known_submit_failure_is_durable_and_a_new_run_can_be_planned() {
    let rig = Rig::new(AdapterMode::RejectSubmit).await;

    let error = rig
        .coordinator
        .execute_approved(4, &rig.run_id)
        .await
        .expect_err("known rejection must fail execution");
    assert!(matches!(
        error,
        GenerationCoordinatorError::Adapter(AdapterError::Rejected(_))
    ));

    let failed = rig.projects.open_project().expect("failed Project");
    assert_eq!(failed.revision(), 6);
    assert_eq!(failed.agent_runs()[0].status(), AgentRunStatus::Failed);

    let replanned = AgentPlanner::new(
        rig.projects.clone(),
        rig.contexts.clone(),
        Arc::new(DeterministicInferenceAdapter),
    )
    .plan(failed.revision())
    .await
    .expect("terminal failure must permit a new Agent Run");
    assert_eq!(replanned.agent_runs().len(), 2);
    assert_eq!(
        replanned.agent_runs()[1].status(),
        AgentRunStatus::AwaitingApproval
    );
}

#[tokio::test]
async fn a_second_plan_is_rejected_while_a_run_is_active() {
    let rig = Rig::new(AdapterMode::RejectSubmit).await;
    let error = AgentPlanner::new(
        rig.projects.clone(),
        rig.contexts.clone(),
        Arc::new(DeterministicInferenceAdapter),
    )
    .plan(4)
    .await
    .expect_err("an approved Run is still active");
    assert!(error.to_string().contains("active Agent Run"));
}

#[tokio::test]
async fn provider_result_must_match_the_planned_candidate_count() {
    let rig = Rig::new(AdapterMode::ReturnOneOfTwo).await;

    let error = rig
        .coordinator
        .execute_approved(4, &rig.run_id)
        .await
        .expect_err("one artifact cannot satisfy a two-Candidate Plan");
    assert!(
        error
            .to_string()
            .contains("expected 2 Candidates, received 1")
    );

    let failed = rig.projects.open_project().expect("failed Project");
    assert_eq!(failed.revision(), 7);
    assert_eq!(failed.agent_runs()[0].status(), AgentRunStatus::Failed);
    assert!(failed.candidates().is_empty());
}

struct Rig {
    _temp: tempfile::TempDir,
    projects: Arc<ProjectService>,
    contexts: Arc<autostudio_provider::context::ContextManager>,
    coordinator: GenerationCoordinator,
    run_id: AgentRunId,
}

impl Rig {
    async fn new(mode: AdapterMode) -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let package = temp.path().join("failure.autostudio");
        let staging = temp.path().join("staging");
        fs::create_dir_all(&staging).expect("staging");
        let store = Arc::new(
            autostudio_storage::SqliteProjectStore::open(&package).expect("project store"),
        );
        let projects = Arc::new(ProjectService::new(store.clone()));
        let contexts = Arc::new(autostudio_provider::context::ContextManager::new(store));
        projects
            .create_project("Failure Contract")
            .expect("project");
        projects
            .set_brief(
                0,
                CreativeBriefDraft {
                    summary: "Short test cue".to_owned(),
                    purpose: None,
                    style: vec![],
                    mood: vec![],
                    instrumentation: vec![],
                    target_duration_seconds: Some(1),
                    lyrics: None,
                    constraints: vec![],
                },
            )
            .expect("brief");
        let planned = AgentPlanner::new(
            projects.clone(),
            contexts.clone(),
            Arc::new(DeterministicInferenceAdapter),
        )
        .plan(1)
        .await
        .expect("plan");
        let run = &planned.agent_runs()[0];
        let run_id = run.id().clone();
        projects
            .approve_agent_run(
                planned.revision(),
                &run_id,
                CostApproval {
                    currency: "USD".to_owned(),
                    max_minor_units: 100,
                    input_hash: run
                        .plan_value()
                        .expect("approved run has a plan")
                        .input_hash()
                        .to_owned(),
                },
            )
            .expect("approval");
        let coordinator = GenerationCoordinator::new(
            projects.clone(),
            Arc::new(FaultAdapter {
                mode,
                staging_root: staging.clone(),
            }),
            Arc::new(ProjectMedia::new(&package, &staging).expect("media")),
        );
        Self {
            _temp: temp,
            projects,
            contexts,
            coordinator,
            run_id,
        }
    }
}

#[derive(Clone, Copy)]
enum AdapterMode {
    RejectSubmit,
    ReturnOneOfTwo,
}

struct FaultAdapter {
    mode: AdapterMode,
    staging_root: PathBuf,
}

impl GenerationAdapter for FaultAdapter {
    fn provider_kind(&self) -> &'static str {
        "fault-music"
    }

    fn model(&self) -> &'static str {
        "contract-v1"
    }

    fn submit(&self, request: GenerationRequest) -> GenerationFuture<'_, GenerationSubmission> {
        Box::pin(async move {
            if matches!(self.mode, AdapterMode::RejectSubmit) {
                return Err(AdapterError::Rejected("request rejected".to_owned()));
            }
            Ok(GenerationSubmission {
                attempt_id: request.attempt_id,
                external_job_id: "job-one-of-two".to_owned(),
            })
        })
    }

    fn observe(&self, _external_job_id: String) -> GenerationFuture<'_, GenerationObservation> {
        Box::pin(async move {
            let output = self.staging_root.join("one.wav");
            write_wav(&output).map_err(|error| AdapterError::Unavailable(error.to_string()))?;
            Ok(GenerationObservation::Succeeded {
                artifacts: vec![GeneratedArtifact {
                    label: "Only Direction".to_owned(),
                    staging_path: output,
                    credits: vec![],
                }],
            })
        })
    }

    fn reconcile(&self, _attempt_id: String) -> GenerationFuture<'_, GenerationReconciliation> {
        Box::pin(async { Ok(GenerationReconciliation::NotFound) })
    }
}

fn write_wav(path: &Path) -> Result<(), hound::Error> {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for _ in 0..48_000 {
        writer.write_sample(0_i32)?;
        writer.write_sample(0_i32)?;
    }
    writer.finalize()
}
