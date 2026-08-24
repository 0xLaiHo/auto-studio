use std::fs;

use autostudio_music_quality::{
    ArtifactRecord, ExperimentRun, ProviderUsage, RunMode, prepare_blind_package,
};
use chrono::Utc;
use sha2::Digest;

#[test]
fn evaluator_package_hides_mode_and_keeps_private_mapping_outside() {
    let temp = tempfile::tempdir().expect("temp directory");
    let evidence = temp.path().join("formal");
    write_candidate(&evidence, RunMode::A, "brief-one", "c-anonymous-a");
    write_candidate(&evidence, RunMode::B, "brief-one", "c-anonymous-b");
    let output = temp.path().join("blind");

    let manifest = prepare_blind_package(&evidence, &output).expect("blind package");

    assert_eq!(manifest.candidates.len(), 2);
    let evaluator =
        fs::read_to_string(output.join("evaluator/manifest.json")).expect("evaluator manifest");
    assert!(!evaluator.contains("mode-a"));
    assert!(!evaluator.contains("mode-b"));
    assert!(!evaluator.contains("\"mode\""));
    let private_map =
        fs::read_to_string(output.join("blind-map.private.json")).expect("private map");
    assert!(private_map.contains("\"mode\": \"a\""));
    assert!(private_map.contains("\"mode\": \"b\""));
    assert!(
        output
            .join("evaluator/c-anonymous-a/composition.mid")
            .is_file()
    );
    assert!(output.join("evaluator/evaluation.csv").is_file());
}

fn write_candidate(root: &std::path::Path, mode: RunMode, brief_id: &str, candidate_id: &str) {
    let mode_name = match mode {
        RunMode::A => "mode-a",
        RunMode::B => "mode-b",
        RunMode::C => "mode-c",
    };
    let output = root.join(mode_name).join(brief_id);
    fs::create_dir_all(&output).expect("candidate directory");
    let artifacts = [
        ("brief.json", br#"{"id":"brief-one"}"#.as_slice()),
        ("spec.json", br#"{"title":"test"}"#.as_slice()),
        ("composition.mid", b"MThd".as_slice()),
    ];
    let artifact_records = artifacts
        .iter()
        .map(|(name, bytes)| {
            fs::write(output.join(name), bytes).expect("artifact");
            ArtifactRecord {
                path: (*name).to_owned(),
                bytes: bytes.len() as u64,
                sha256: hex::encode(sha2::Sha256::digest(bytes)),
            }
        })
        .collect();
    let now = Utc::now();
    let run = ExperimentRun {
        schema_version: "q0-run-v1".to_owned(),
        run_id: format!("{brief_id}-{mode_name}"),
        candidate_id: candidate_id.to_owned(),
        status: "completed".to_owned(),
        mode,
        brief_id: brief_id.to_owned(),
        brief_level: "L1".to_owned(),
        started_at: now,
        completed_at: now,
        provider: "deepseek".to_owned(),
        model: "deepseek-v4-pro".to_owned(),
        thinking_level: "high".to_owned(),
        schema_valid: true,
        compiled: true,
        validation_error: None,
        turn_count: 1,
        total_usage: ProviderUsage::default(),
        total_latency_ms: 1,
        artifacts: artifact_records,
    };
    fs::write(
        output.join("run.json"),
        serde_json::to_vec_pretty(&run).expect("run JSON"),
    )
    .expect("run record");
}
