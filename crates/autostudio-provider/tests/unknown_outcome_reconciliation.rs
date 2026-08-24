use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use autostudio_core::agent::{AgentRunStatus, CostApproval};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_media::ProjectMedia;
use autostudio_provider::{
    AdapterError, AgentPlanner, DeterministicInferenceAdapter, GeneratedArtifact,
    GenerationAdapter, GenerationCoordinator, GenerationFuture, GenerationObservation,
    GenerationReconciliation, GenerationRequest, GenerationSubmission,
};

#[tokio::test]
async fn unknown_submit_is_reconciled_without_a_second_chargeable_submit() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("unknown.autostudio");
    let staging = temp.path().join("staging");
    fs::create_dir_all(&staging).expect("staging");
    let projects = Arc::new(ProjectService::new(Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("project store"),
    )));
    projects.create_project("Night Drive").expect("project");
    projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "Nocturnal synthwave cue".to_owned(),
                purpose: None,
                style: vec!["synthwave".to_owned()],
                mood: vec!["tense".to_owned()],
                instrumentation: vec!["analog synth".to_owned()],
                target_duration_seconds: Some(1),
                lyrics: None,
                constraints: vec![],
            },
        )
        .expect("brief");
    let planned = AgentPlanner::new(projects.clone(), Arc::new(DeterministicInferenceAdapter))
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
                input_hash: run.plan_value().input_hash().to_owned(),
            },
        )
        .expect("approval");
    let adapter = Arc::new(AmbiguousAdapter::new(
        &staging,
        ReconciliationMode::Succeeded,
    ));
    let coordinator = GenerationCoordinator::new(
        projects.clone(),
        adapter.clone(),
        Arc::new(ProjectMedia::new(&package, &staging).expect("media")),
    );

    let error = coordinator
        .execute_approved(3, &run_id)
        .await
        .expect_err("submit response must be ambiguous");
    assert!(matches!(
        error,
        autostudio_provider::GenerationCoordinatorError::Adapter(AdapterError::UnknownOutcome(_))
    ));
    assert_eq!(adapter.submit_calls.load(Ordering::SeqCst), 1);
    let unknown = projects.open_project().expect("unknown Project");
    assert_eq!(unknown.revision(), 5);
    assert_eq!(
        unknown.agent_runs()[0].status(),
        AgentRunStatus::UnknownOutcome
    );

    coordinator
        .execute_approved(5, &run_id)
        .await
        .expect_err("Unknown Outcome cannot be submitted again");
    assert_eq!(adapter.submit_calls.load(Ordering::SeqCst), 1);

    let reconciled = coordinator
        .reconcile_unknown(5, &run_id)
        .await
        .expect("reconciliation");
    assert_eq!(adapter.submit_calls.load(Ordering::SeqCst), 1);
    assert_eq!(reconciled.revision(), 7);
    assert_eq!(
        reconciled.agent_runs()[0].status(),
        AgentRunStatus::Completed
    );
    assert_eq!(reconciled.candidates().len(), 2);
}

#[tokio::test]
async fn provider_confirmed_absence_closes_unknown_attempt_without_resubmitting() {
    let rig = prepare_unknown(ReconciliationMode::NotFound).await;
    let reconciled = rig
        .coordinator
        .reconcile_unknown(5, &rig.run_id)
        .await
        .expect("not-found reconciliation");
    assert_eq!(reconciled.revision(), 6);
    assert_eq!(reconciled.agent_runs()[0].status(), AgentRunStatus::Failed);
    assert_eq!(rig.adapter.submit_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn provider_confirmed_acceptance_restores_submitted_job_without_resubmitting() {
    let rig = prepare_unknown(ReconciliationMode::Accepted).await;
    let reconciled = rig
        .coordinator
        .reconcile_unknown(5, &rig.run_id)
        .await
        .expect("accepted reconciliation");
    assert_eq!(reconciled.revision(), 6);
    assert_eq!(
        reconciled.agent_runs()[0].status(),
        AgentRunStatus::Submitted
    );
    let still_pending = rig
        .coordinator
        .resume_submitted(6, &rig.run_id)
        .await
        .expect("poll accepted Job");
    assert_eq!(still_pending.revision(), 6);
    assert_eq!(rig.adapter.submit_calls.load(Ordering::SeqCst), 1);
}

struct UnknownRig {
    _temp: tempfile::TempDir,
    adapter: Arc<AmbiguousAdapter>,
    coordinator: GenerationCoordinator,
    run_id: autostudio_core::agent::AgentRunId,
}

async fn prepare_unknown(mode: ReconciliationMode) -> UnknownRig {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("unknown-mode.autostudio");
    let staging = temp.path().join("staging");
    fs::create_dir_all(&staging).expect("staging");
    let projects = Arc::new(ProjectService::new(Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("project store"),
    )));
    projects.create_project("Unknown Mode").expect("project");
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
    let planned = AgentPlanner::new(projects.clone(), Arc::new(DeterministicInferenceAdapter))
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
                input_hash: run.plan_value().input_hash().to_owned(),
            },
        )
        .expect("approval");
    let adapter = Arc::new(AmbiguousAdapter::new(&staging, mode));
    let coordinator = GenerationCoordinator::new(
        projects,
        adapter.clone(),
        Arc::new(ProjectMedia::new(&package, &staging).expect("media")),
    );
    coordinator
        .execute_approved(3, &run_id)
        .await
        .expect_err("submit response must be ambiguous");
    UnknownRig {
        _temp: temp,
        adapter,
        coordinator,
        run_id,
    }
}

#[derive(Clone, Copy)]
enum ReconciliationMode {
    NotFound,
    Accepted,
    Succeeded,
}

struct AmbiguousAdapter {
    staging_root: PathBuf,
    attempts: Mutex<Option<GenerationSubmission>>,
    submit_calls: AtomicUsize,
    provider_kind: &'static str,
    model: &'static str,
    reconciliation: ReconciliationMode,
}

impl AmbiguousAdapter {
    fn new(staging_root: &Path, reconciliation: ReconciliationMode) -> Self {
        Self {
            staging_root: staging_root.to_owned(),
            attempts: Mutex::new(None),
            submit_calls: AtomicUsize::new(0),
            provider_kind: "ambiguous-music",
            model: "test-v1",
            reconciliation,
        }
    }
}

impl GenerationAdapter for AmbiguousAdapter {
    fn provider_kind(&self) -> &str {
        self.provider_kind
    }

    fn model(&self) -> &str {
        self.model
    }

    fn submit(&self, request: GenerationRequest) -> GenerationFuture<'_, GenerationSubmission> {
        Box::pin(async move {
            self.submit_calls.fetch_add(1, Ordering::SeqCst);
            let submission = GenerationSubmission {
                attempt_id: request.attempt_id,
                external_job_id: "accepted-job-1".to_owned(),
            };
            *self.attempts.lock().expect("attempt ledger") = Some(submission);
            Err(AdapterError::UnknownOutcome(
                "connection closed after upload".to_owned(),
            ))
        })
    }

    fn observe(&self, _external_job_id: String) -> GenerationFuture<'_, GenerationObservation> {
        Box::pin(async { Ok(GenerationObservation::Pending) })
    }

    fn reconcile(&self, attempt_id: String) -> GenerationFuture<'_, GenerationReconciliation> {
        Box::pin(async move {
            let submission = self
                .attempts
                .lock()
                .expect("attempt ledger")
                .clone()
                .filter(|submission| submission.attempt_id == attempt_id)
                .ok_or_else(|| AdapterError::Rejected("attempt not found".to_owned()))?;
            match self.reconciliation {
                ReconciliationMode::NotFound => Ok(GenerationReconciliation::NotFound),
                ReconciliationMode::Accepted => {
                    Ok(GenerationReconciliation::Accepted { submission })
                }
                ReconciliationMode::Succeeded => {
                    let first = self.staging_root.join("reconciled-a.wav");
                    let second = self.staging_root.join("reconciled-b.wav");
                    write_wav(&first)
                        .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
                    write_wav(&second)
                        .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
                    Ok(GenerationReconciliation::Succeeded {
                        submission,
                        artifacts: vec![
                            GeneratedArtifact {
                                label: "Recovered Direction A".to_owned(),
                                staging_path: first,
                                credits: vec![],
                            },
                            GeneratedArtifact {
                                label: "Recovered Direction B".to_owned(),
                                staging_path: second,
                                credits: vec![],
                            },
                        ],
                    })
                }
            }
        })
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
