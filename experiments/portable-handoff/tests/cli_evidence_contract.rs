use std::fs;

use autostudio_portable_handoff::EvidenceManifest;
use sha2::{Digest, Sha256};

#[test]
fn compile_command_writes_three_hashed_artifacts_without_credentials() {
    let output = tempfile::tempdir().expect("temporary output");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../music-quality/evidence/pilot/l1-song-hook/spec.json");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("autostudio-portable-handoff");
    command
        .arg("compile")
        .arg("--input")
        .arg(input)
        .arg("--output-dir")
        .arg(output.path())
        .assert()
        .success();

    let manifest: EvidenceManifest =
        serde_json::from_slice(&fs::read(output.path().join("manifest.json")).expect("manifest"))
            .expect("valid manifest");
    assert_eq!(manifest.artifacts.len(), 3);
    for artifact in &manifest.artifacts {
        let bytes = fs::read(output.path().join(&artifact.path)).expect("artifact");
        assert_eq!(artifact.bytes, u64::try_from(bytes.len()).expect("size"));
        assert_eq!(artifact.sha256, hex::encode(Sha256::digest(&bytes)));
    }

    let assignments =
        fs::read_to_string(output.path().join("instrument-assignments.json")).expect("assignments");
    assert!(assignments.contains("gm.square-lead"));
    assert!(!assignments.contains("API_KEY"));
}
