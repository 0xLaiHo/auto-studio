use std::sync::{Arc, Mutex};

use autostudio_core::agent::{
    AgentDecision, AgentPlanDraft, AgentRunStatus, CostApproval, CostEstimate,
    GenerationAttemptDraft, GenerationIntent, GenerationJobDraft, InferenceProvenance,
    InferenceUsage,
};
use autostudio_core::production::{
    AssetVersionDraft, AudioMetadata, CandidateDraft, HandoffExportDraft, HandoffFile,
    HandoffRequest, HandoffSink, ProvenanceRecord, RightsDeclaration,
};
use autostudio_core::project::{CreativeBriefDraft, ProjectService};

#[test]
#[allow(clippy::too_many_lines)]
fn generated_candidate_requires_explicit_selection_before_entering_the_timeline() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("production.autostudio"))
            .expect("open project store"),
    );
    let projects = ProjectService::new(store);
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
                target_duration_seconds: Some(60),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");
    let planned = projects
        .plan_agent_run(
            1,
            AgentPlanDraft {
                visible_summary: "Generate one direction".to_owned(),
                decision: AgentDecision::GenerateMusic(GenerationIntent {
                    prompt: "nocturnal synthwave".to_owned(),
                    duration_seconds: 60,
                    candidate_count: 1,
                }),
                estimated_cost: CostEstimate::Known {
                    currency: "USD".to_owned(),
                    lower_minor_units: 40,
                    upper_minor_units: 80,
                },
                usage: InferenceUsage::default(),
                inference: InferenceProvenance::default(),
                input_hash: "sha256:brief-1".to_owned(),
            },
        )
        .expect("plan");
    let run_id = planned.agent_runs()[0].id().clone();
    projects
        .approve_agent_run(
            2,
            &run_id,
            CostApproval {
                currency: "USD".to_owned(),
                max_minor_units: 80,
                input_hash: "sha256:brief-1".to_owned(),
            },
        )
        .expect("approval");
    projects
        .prepare_generation(
            3,
            &run_id,
            GenerationAttemptDraft {
                attempt_id: "attempt-1".to_owned(),
                provider_kind: "fake-music".to_owned(),
                model: "deterministic-v1".to_owned(),
                request_hash: "sha256:brief-1".to_owned(),
            },
        )
        .expect("prepare generation");
    projects
        .record_generation_submitted(
            4,
            &run_id,
            GenerationJobDraft {
                attempt_id: "attempt-1".to_owned(),
                external_job_id: "provider-job-42".to_owned(),
                provider_kind: "fake-music".to_owned(),
                model: "deterministic-v1".to_owned(),
                request_hash: "sha256:brief-1".to_owned(),
            },
        )
        .expect("submit generation");

    let generated = projects
        .commit_candidates(
            5,
            &run_id,
            vec![candidate(
                "Direction A",
                "assets/a.wav",
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )],
        )
        .expect("commit Candidate");
    assert_eq!(
        generated.agent_runs()[0].status(),
        AgentRunStatus::Completed
    );
    assert_eq!(generated.candidates().len(), 1);
    assert!(generated.selection().is_none());
    assert!(generated.timeline().clips().is_empty());

    let candidate_id = generated.candidates()[0].id().clone();
    let selected = projects
        .select_candidate(6, &candidate_id, 0)
        .expect("Creator Selection");
    assert_eq!(
        selected.selection().expect("Selection").candidate_id(),
        &candidate_id
    );
    assert_eq!(selected.timeline().clips().len(), 1);
    assert_eq!(selected.timeline().clips()[0].start_micros(), 0);

    let observed = Arc::new(Mutex::new(None));
    let handed_off = projects
        .export_handoff(
            7,
            &RecordingHandoffSink {
                observed: observed.clone(),
            },
        )
        .expect("DAW Handoff");
    let request = observed
        .lock()
        .expect("request mutex")
        .clone()
        .expect("request");
    assert_eq!(request.source_project_revision(), 7);
    assert_eq!(request.asset().relative_path(), "assets/a.wav");
    assert_eq!(handed_off.exports().len(), 1);
    assert_eq!(handed_off.exports()[0].source_project_revision(), 7);
    assert_eq!(projects.open_project().expect("reopen"), handed_off);
}

struct RecordingHandoffSink {
    observed: Arc<Mutex<Option<HandoffRequest>>>,
}

impl HandoffSink for RecordingHandoffSink {
    fn export(&self, request: &HandoffRequest) -> Result<HandoffExportDraft, String> {
        *self.observed.lock().map_err(|error| error.to_string())? = Some(request.clone());
        Ok(HandoffExportDraft {
            relative_path: "exports/handoff-r7".to_owned(),
            manifest_sha256:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            files: vec![HandoffFile {
                relative_path: "audio/selected.wav".to_owned(),
                sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
                media_type: "audio/wav".to_owned(),
            }],
        })
    }
}

fn candidate(label: &str, relative_path: &str, sha256: &str) -> CandidateDraft {
    CandidateDraft {
        label: label.to_owned(),
        asset: AssetVersionDraft {
            relative_path: relative_path.to_owned(),
            sha256: sha256.to_owned(),
            media_type: "audio/wav".to_owned(),
            audio: AudioMetadata {
                sample_rate_hz: 48_000,
                channels: 2,
                duration_micros: 60_000_000,
                bit_depth: 24,
            },
            provenance: ProvenanceRecord {
                provider_kind: "fake-music".to_owned(),
                model: "deterministic-v1".to_owned(),
                adapter_version: "0.1.0".to_owned(),
                external_job_id: Some("provider-job-42".to_owned()),
                input_hash: "sha256:brief-1".to_owned(),
                rights: RightsDeclaration::CreatorOwned,
                credits: vec![],
            },
        },
        note: Some("Stronger opening".to_owned()),
    }
}
