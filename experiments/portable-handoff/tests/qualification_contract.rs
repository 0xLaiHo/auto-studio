use std::fs;
use std::path::{Path, PathBuf};

use autostudio_music_quality::ExperimentalMusicSpec;
use autostudio_portable_handoff::{
    CheckStatus, EvidenceArtifact, MarkerObservation, ProgramChangeObservation,
    QualificationChecks, QualificationEvidence, QualificationOutcome, QualificationResults,
    QualificationTarget, QualificationTargets, TargetReadiness, prepare_qualification_matrix,
    verify_qualification_matrix, write_portable_handoff,
};
use midly::{MidiMessage, Smf, TrackEventKind, num::u7};
use sha2::{Digest, Sha256};

#[test]
fn prepares_and_verifies_an_honest_not_run_matrix() {
    let fixture = Fixture::new();
    let targets_file = fixture.write_targets(&blocked_targets());

    let (plan, results) =
        prepare_qualification_matrix(&fixture.handoff_dir, &targets_file, &fixture.matrix_dir)
            .expect("prepare matrix");

    assert_eq!(plan.targets.len(), 3);
    assert_eq!(results.results.len(), 3);
    assert!(
        results
            .results
            .iter()
            .all(|result| result.outcome == QualificationOutcome::NotRun)
    );

    let summary = verify_qualification_matrix(
        &fixture.handoff_dir,
        &fixture.matrix_dir.join("qualification-plan.json"),
        &fixture.matrix_dir.join("qualification-results.json"),
        &fixture.matrix_dir,
        &fixture.matrix_dir.join("qualification-summary.json"),
    )
    .expect("verify not-run matrix");
    assert_eq!(summary.total, 3);
    assert_eq!(summary.not_run, 3);
    assert!(!summary.all_required_targets_passed);
}

#[test]
fn rejects_a_handoff_artifact_changed_after_manifest_creation() {
    let fixture = Fixture::new();
    let targets_file = fixture.write_targets(&blocked_targets());
    fs::write(fixture.handoff_dir.join("composition.mid"), b"tampered").expect("tamper MIDI");

    let error =
        prepare_qualification_matrix(&fixture.handoff_dir, &targets_file, &fixture.matrix_dir)
            .expect_err("tampered handoff must fail");

    assert!(error.to_string().contains("size does not match"));
}

#[test]
fn a_blocked_target_cannot_be_claimed_as_passed() {
    let fixture = Fixture::new();
    let targets_file = fixture.write_targets(&blocked_targets());
    prepare_qualification_matrix(&fixture.handoff_dir, &targets_file, &fixture.matrix_dir)
        .expect("prepare matrix");
    let results_path = fixture.matrix_dir.join("qualification-results.json");
    let mut results: QualificationResults =
        serde_json::from_slice(&fs::read(&results_path).expect("results")).expect("valid results");
    results.results[0].outcome = QualificationOutcome::Pass;
    fs::write(
        &results_path,
        serde_json::to_vec_pretty(&results).expect("serialize results"),
    )
    .expect("write results");

    let error = verify_qualification_matrix(
        &fixture.handoff_dir,
        &fixture.matrix_dir.join("qualification-plan.json"),
        &results_path,
        &fixture.matrix_dir,
        &fixture.matrix_dir.join("qualification-summary.json"),
    )
    .expect_err("blocked target cannot pass");

    assert!(error.to_string().contains("blocked target"));
}

#[test]
fn accepts_a_ready_target_only_with_complete_changed_midi_evidence() {
    let fixture = Fixture::new();
    let targets_file = fixture.write_targets(&QualificationTargets {
        schema_version: "daw-qualification-targets-v1".to_owned(),
        frozen_at: "2026-08-25".to_owned(),
        targets: vec![QualificationTarget {
            id: "test-daw".to_owned(),
            product: "Test DAW".to_owned(),
            exact_version: Some("1.2.3".to_owned()),
            platform: Some("test-platform".to_owned()),
            readiness: TargetReadiness::Ready,
            blocked_reason: None,
            required_for_mvp: true,
        }],
    });
    prepare_qualification_matrix(&fixture.handoff_dir, &targets_file, &fixture.matrix_dir)
        .expect("prepare matrix");

    let evidence_dir = fixture.root.path().join("evidence");
    fs::create_dir_all(&evidence_dir).expect("evidence directory");
    let screenshot = [
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, b't', b'e', b's', b't',
    ];
    fs::write(evidence_dir.join("screen.png"), screenshot).expect("screenshot");
    fs::write(evidence_dir.join("project.dawproject"), b"saved project").expect("project");
    let edited_midi = changed_midi(&fixture.handoff_dir.join("composition.mid"));
    fs::write(evidence_dir.join("edited.mid"), &edited_midi).expect("edited MIDI");

    let results_path = fixture.matrix_dir.join("qualification-results.json");
    let mut results: QualificationResults =
        serde_json::from_slice(&fs::read(&results_path).expect("results")).expect("valid results");
    let result = &mut results.results[0];
    result.outcome = QualificationOutcome::Pass;
    result.observed_version = Some("1.2.3".to_owned());
    result.executable_sha256 = Some(sha256(b"test executable"));
    result.checks = Some(QualificationChecks {
        import_without_repair: CheckStatus::Passed,
        semantic_tracks_preserved: CheckStatus::Passed,
        tempo_meter_preserved: CheckStatus::Passed,
        midi_events_preserved_before_edit: CheckStatus::Passed,
        markers: MarkerObservation::NotExposed,
        program_change: ProgramChangeObservation::Ignored,
        save_reopen: CheckStatus::Passed,
        intentional_edit_export: CheckStatus::Passed,
    });
    result.evidence = Some(QualificationEvidence {
        screenshot: evidence_artifact("screen.png", &screenshot),
        saved_project: evidence_artifact("project.dawproject", b"saved project"),
        edited_midi: evidence_artifact("edited.mid", &edited_midi),
    });
    fs::write(
        &results_path,
        serde_json::to_vec_pretty(&results).expect("serialize results"),
    )
    .expect("write results");

    let summary = verify_qualification_matrix(
        &fixture.handoff_dir,
        &fixture.matrix_dir.join("qualification-plan.json"),
        &results_path,
        &evidence_dir,
        &fixture.matrix_dir.join("qualification-summary.json"),
    )
    .expect("complete evidence should pass");

    assert_eq!(summary.passed, 1);
    assert!(summary.all_required_targets_passed);
}

struct Fixture {
    root: tempfile::TempDir,
    handoff_dir: PathBuf,
    matrix_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary root");
        let handoff_dir = root.path().join("handoff");
        let matrix_dir = root.path().join("matrix");
        let spec = ExperimentalMusicSpec::parse_and_validate(pilot_fixture()).expect("valid Pilot");
        write_portable_handoff(&handoff_dir, &spec).expect("portable handoff");
        Self {
            root,
            handoff_dir,
            matrix_dir,
        }
    }

    fn write_targets(&self, targets: &QualificationTargets) -> PathBuf {
        let path = self.root.path().join("targets.json");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&targets).expect("serialize targets"),
        )
        .expect("write targets");
        path
    }
}

fn blocked_targets() -> QualificationTargets {
    QualificationTargets {
        schema_version: "daw-qualification-targets-v1".to_owned(),
        frozen_at: "2026-08-25".to_owned(),
        targets: [
            ("steinberg-cubase", "Steinberg Cubase"),
            ("presonus-studio-one-pro", "PreSonus Studio One Pro"),
            ("image-line-fl-studio", "Image-Line FL Studio"),
        ]
        .into_iter()
        .map(|(id, product)| QualificationTarget {
            id: id.to_owned(),
            product: product.to_owned(),
            exact_version: None,
            platform: None,
            readiness: TargetReadiness::Blocked,
            blocked_reason: Some("not installed".to_owned()),
            required_for_mvp: true,
        })
        .collect(),
    }
}

fn changed_midi(source: &Path) -> Vec<u8> {
    let source = fs::read(source).expect("source MIDI");
    let mut smf = Smf::parse(&source).expect("valid source MIDI");
    let mut changed = false;
    for track in &mut smf.tracks {
        for event in track {
            if let TrackEventKind::Midi {
                message: MidiMessage::NoteOn { key, .. },
                ..
            } = &mut event.kind
            {
                *key = u7::new((key.as_int() + 1).min(127));
                changed = true;
                break;
            }
        }
        if changed {
            break;
        }
    }
    assert!(changed, "Pilot MIDI contains a note-on event");
    let mut output = Vec::new();
    smf.write_std(&mut output).expect("write changed MIDI");
    output
}

fn evidence_artifact(path: &str, bytes: &[u8]) -> EvidenceArtifact {
    EvidenceArtifact {
        path: path.to_owned(),
        bytes: u64::try_from(bytes.len()).expect("artifact size"),
        sha256: sha256(bytes),
    }
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn pilot_fixture() -> &'static str {
    include_str!("../../music-quality/evidence/pilot/l1-song-hook/spec.json")
}
