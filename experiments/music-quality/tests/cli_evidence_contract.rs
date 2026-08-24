use std::fs;

use assert_cmd::Command;

#[test]
fn compile_command_writes_hashed_reproducible_artifacts_without_credentials() {
    let temp = tempfile::tempdir().expect("temp directory");
    let input = temp.path().join("input.json");
    let output = temp.path().join("evidence");
    fs::write(&input, fixture()).expect("write fixture");

    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .env("DEEPSEEK_API_KEY", "must-never-appear-in-artifacts")
        .args([
            "compile",
            "--input",
            input.to_str().expect("input path"),
            "--output-dir",
            output.to_str().expect("output path"),
        ])
        .assert()
        .success();

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).expect("manifest exists"))
            .expect("manifest JSON");
    assert_eq!(manifest["schema_version"], "q0-evidence-v1");
    assert_eq!(manifest["artifacts"].as_array().map(Vec::len), Some(2));

    for artifact in manifest["artifacts"].as_array().expect("artifact list") {
        let path = output.join(artifact["path"].as_str().expect("artifact path"));
        let bytes = fs::read(path).expect("artifact exists");
        assert_eq!(artifact["bytes"].as_u64(), Some(bytes.len() as u64));
        assert_eq!(artifact["sha256"].as_str().map(str::len), Some(64));
    }

    for entry in fs::read_dir(&output).expect("evidence directory") {
        let bytes = fs::read(entry.expect("directory entry").path()).expect("read artifact");
        assert!(!String::from_utf8_lossy(&bytes).contains("must-never-appear-in-artifacts"));
    }
}

fn fixture() -> &'static str {
    r#"{
      "title": "CLI evidence contract",
      "tempo_map": [{"bar": 1, "bpm": 120.0, "time_signature": {"numerator": 4, "denominator": 4}}],
      "key_map": [{"bar": 1, "tonic": "C", "mode": "major"}],
      "sections": [{"id": "a", "label": "A", "start_bar": 1, "length_bars": 2, "intent": "state motif"}],
      "tracks": [{
        "id": "lead", "name": "Lead", "role": "melody",
        "register": {"low": 48, "high": 84}, "instrument_hint": "electric piano",
        "regions": [{"section_id": "a", "notes": [{"beat": 0.0, "duration": 1.0, "pitch": 60, "velocity": 96}], "cc": []}]
      }]
    }"#
}
