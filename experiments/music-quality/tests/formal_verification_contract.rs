use std::fs;

use autostudio_music_quality::{
    ArtifactRecord, ExperimentRun, ProviderUsage, RunMode, verify_formal,
    verify_formal_with_protocol,
};
use chrono::Utc;
use sha2::{Digest, Sha256};

#[test]
fn verifies_exact_frozen_candidate_set_and_computes_peak_cost() {
    let temp = tempfile::tempdir().expect("temp directory");
    let assets = temp.path().join("assets");
    let evidence = temp.path().join("formal");
    fs::create_dir_all(assets.join("environment")).expect("assets");
    fs::write(
        assets.join("protocol.lock.json"),
        r#"{
          "provider":{"name":"deepseek","model_id":"deepseek-v4-pro","thinking_level":"high"},
          "modes":{"a_brief_ids":["a-one"],"b_brief_ids":["b-one"]},
          "gates":{"mode_b_valid_and_compiled_minimum":"1/1"}
        }"#,
    )
    .expect("protocol");
    fs::write(
        assets.join("environment/deepseek-pricing-v1.json"),
        r#"{
          "off_peak":{"input_cache_hit":1.0,"input_cache_miss":2.0,"output":3.0},
          "peak":{"input_cache_hit":2.0,"input_cache_miss":4.0,"output":6.0}
        }"#,
    )
    .expect("pricing");
    write_run(&evidence, RunMode::A, "a-one", "c-a");
    write_run(&evidence, RunMode::B, "b-one", "c-b");

    let summary = verify_formal(&assets, &evidence).expect("formal verification");

    assert_eq!(summary.observed_candidates, 2);
    assert_eq!(summary.mode_b_valid_and_compiled, 1);
    assert!(summary.mode_b_device_gate_passed);
    assert_eq!(summary.total_usage.prompt_cache_hit_tokens, Some(20));
    assert_eq!(summary.total_usage.prompt_cache_miss_tokens, Some(40));
    assert!((summary.peak_cost_usd - 0.000_8).abs() < f64::EPSILON);
}

#[test]
fn rejects_a_missing_frozen_candidate() {
    let temp = tempfile::tempdir().expect("temp directory");
    let assets = temp.path().join("assets");
    let evidence = temp.path().join("formal");
    fs::create_dir_all(assets.join("environment")).expect("assets");
    fs::write(
        assets.join("protocol.lock.json"),
        r#"{
          "provider":{"name":"deepseek","model_id":"deepseek-v4-pro","thinking_level":"high"},
          "modes":{"a_brief_ids":["missing"],"b_brief_ids":[]},
          "gates":{"mode_b_valid_and_compiled_minimum":"0/0"}
        }"#,
    )
    .expect("protocol");
    fs::write(
        assets.join("environment/deepseek-pricing-v1.json"),
        r#"{"off_peak":{"input_cache_hit":1,"input_cache_miss":1,"output":1},"peak":{"input_cache_hit":1,"input_cache_miss":1,"output":1}}"#,
    )
    .expect("pricing");

    let error = verify_formal(&assets, &evidence).expect_err("missing candidate");

    assert!(
        error
            .to_string()
            .contains("missing formal run mode=a brief=missing")
    );
}

#[test]
fn rejects_frozen_input_hash_drift_before_reading_runs() {
    let temp = tempfile::tempdir().expect("temp directory");
    let assets = temp.path().join("assets");
    let evidence = temp.path().join("formal");
    fs::create_dir_all(assets.join("environment")).expect("assets");
    fs::write(assets.join("frozen.txt"), "changed").expect("frozen input");
    fs::write(
        assets.join("protocol.lock.json"),
        r#"{
          "provider":{"name":"deepseek","model_id":"deepseek-v4-pro","thinking_level":"high"},
          "modes":{"a_brief_ids":[],"b_brief_ids":[]},
          "gates":{"mode_b_valid_and_compiled_minimum":"0/0"},
          "input_hashes":{"frozen.txt":"0000000000000000000000000000000000000000000000000000000000000000"}
        }"#,
    )
    .expect("protocol");
    fs::write(
        assets.join("environment/deepseek-pricing-v1.json"),
        r#"{"off_peak":{"input_cache_hit":1,"input_cache_miss":1,"output":1},"peak":{"input_cache_hit":1,"input_cache_miss":1,"output":1}}"#,
    )
    .expect("pricing");

    let error = verify_formal(&assets, &evidence).expect_err("hash drift");

    assert!(error.to_string().contains("frozen input hash mismatch"));
}

#[test]
fn bound_protocol_rejects_a_run_without_protocol_binding_evidence() {
    let temp = tempfile::tempdir().expect("temp directory");
    let assets = temp.path().join("assets");
    let evidence = temp.path().join("formal");
    fs::create_dir_all(assets.join("environment")).expect("assets");
    let protocol = assets.join("protocol-v3.lock.json");
    fs::write(
        &protocol,
        r#"{
          "schema_version":"q0-protocol-v3-test",
          "run_binding_required":true,
          "provider":{"name":"deepseek","model_id":"deepseek-v4-pro","thinking_level":"high"},
          "modes":{"a_brief_ids":[],"b_brief_ids":["b-one"]},
          "mode_b_resource_repair":{"max_turns":1},
          "gates":{"mode_b_valid_and_compiled_minimum":"1/1"}
        }"#,
    )
    .expect("protocol");
    fs::write(
        assets.join("environment/deepseek-pricing-v1.json"),
        r#"{"off_peak":{"input_cache_hit":1,"input_cache_miss":1,"output":1},"peak":{"input_cache_hit":1,"input_cache_miss":1,"output":1}}"#,
    )
    .expect("pricing");
    write_run(&evidence, RunMode::B, "b-one", "c-b");

    let error = verify_formal_with_protocol(&assets, &evidence, &protocol)
        .expect_err("missing binding must fail");

    assert!(
        error
            .to_string()
            .contains("missing protocol binding artifact")
    );
}

fn write_run(root: &std::path::Path, mode: RunMode, brief_id: &str, candidate_id: &str) {
    let mode_dir = match mode {
        RunMode::A => "mode-a",
        RunMode::B => "mode-b",
        RunMode::C => "mode-c",
    };
    let output = root.join(mode_dir).join(brief_id);
    fs::create_dir_all(&output).expect("run directory");
    let spec = valid_spec();
    let parsed = autostudio_music_quality::ExperimentalMusicSpec::parse_and_validate(spec)
        .expect("valid spec");
    let midi = autostudio_music_quality::compile_to_smf(&parsed).expect("MIDI");
    let artifacts = [
        ("spec.json", spec.as_bytes()),
        ("composition.mid", midi.as_slice()),
    ]
    .into_iter()
    .map(|(name, bytes)| {
        fs::write(output.join(name), bytes).expect("artifact");
        ArtifactRecord {
            path: name.to_owned(),
            bytes: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
        }
    })
    .collect();
    let now = Utc::now();
    let run = ExperimentRun {
        schema_version: "q0-run-v1".to_owned(),
        run_id: format!("{brief_id}-{mode_dir}"),
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
        total_usage: ProviderUsage {
            prompt_tokens: Some(30),
            prompt_cache_hit_tokens: Some(10),
            prompt_cache_miss_tokens: Some(20),
            completion_tokens: Some(50),
            total_tokens: Some(80),
        },
        total_latency_ms: 100,
        artifacts,
    };
    fs::write(
        output.join("run.json"),
        serde_json::to_vec_pretty(&run).expect("run JSON"),
    )
    .expect("run record");
}

fn valid_spec() -> &'static str {
    r#"{
      "title":"fixture",
      "tempo_map":[{"bar":1,"bpm":120,"time_signature":{"numerator":4,"denominator":4}}],
      "key_map":[{"bar":1,"tonic":"C","mode":"major"}],
      "sections":[{"id":"a","label":"A","start_bar":1,"length_bars":1,"intent":"test"}],
      "tracks":[{"id":"lead","name":"Lead","role":"melody","register":{"low":48,"high":84},"instrument_hint":"piano","regions":[{"section_id":"a","notes":[{"beat":0,"duration":1,"pitch":60,"velocity":90}],"cc":[]}]}]
    }"#
}
