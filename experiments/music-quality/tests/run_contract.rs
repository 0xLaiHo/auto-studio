use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use assert_cmd::Command;
use serde_json::json;

#[test]
fn mode_a_run_writes_provider_spec_midi_and_hashed_run_record() {
    let response = json!({
        "id": "run-contract-response",
        "model": "deepseek-v4-pro",
        "choices": [{"finish_reason": "stop", "message": {
            "content": valid_spec(),
            "reasoning_content": "must-not-be-persisted"
        }}],
        "usage": {"prompt_tokens": 500, "completion_tokens": 300, "total_tokens": 800}
    });
    let (base_url, server) = serve_responses(vec![response]);
    let temp = tempfile::tempdir().expect("temp directory");
    let output = temp.path().join("run");

    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .env("DEEPSEEK_API_KEY", "run-contract-secret")
        .env("DEEPSEEK_BASE_URL", base_url)
        .env("DEEPSEEK_MODEL", "deepseek-v4-pro")
        .args([
            "run",
            "--mode",
            "a",
            "--brief-id",
            "l1-song-hook",
            "--output-dir",
            output.to_str().expect("output path"),
        ])
        .assert()
        .success();
    server.join().expect("fixture server");

    let run: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("run.json")).expect("run record"))
            .expect("run JSON");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["mode"], "a");
    assert_eq!(run["brief_id"], "l1-song-hook");
    assert_eq!(run["schema_valid"], true);
    assert_eq!(run["compiled"], true);
    assert_eq!(run["total_usage"]["total_tokens"], 800);
    assert!(output.join("composition.mid").is_file());
    assert!(output.join("spec.json").is_file());
    assert!(output.join("turn-01.json").is_file());

    for entry in fs::read_dir(&output).expect("run directory") {
        let content = fs::read(entry.expect("entry").path()).expect("artifact");
        let text = String::from_utf8_lossy(&content);
        assert!(!text.contains("run-contract-secret"));
        assert!(!text.contains("must-not-be-persisted"));
    }
}

#[test]
fn mode_b_persists_each_completed_turn_before_a_later_provider_failure() {
    let skeleton = json!({
        "id": "skeleton-response",
        "model": "deepseek-v4-pro",
        "choices": [{"finish_reason": "stop", "message": {
            "content": "{\"title\":\"skeleton\",\"tempo_map\":[],\"key_map\":[],\"sections\":[],\"track_plan\":[]}"
        }}],
        "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150}
    });
    let malformed_second_turn = json!({
        "id": "malformed-response",
        "choices": [{"finish_reason": "stop", "message": {}}]
    });
    let (base_url, server) = serve_responses(vec![skeleton, malformed_second_turn]);
    let temp = tempfile::tempdir().expect("temp directory");
    let output = temp.path().join("run");

    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .env("DEEPSEEK_API_KEY", "run-contract-secret")
        .env("DEEPSEEK_BASE_URL", base_url)
        .env("DEEPSEEK_MODEL", "deepseek-v4-pro")
        .args([
            "run",
            "--mode",
            "b",
            "--brief-id",
            "l1-song-hook",
            "--output-dir",
            output.to_str().expect("output path"),
        ])
        .assert()
        .failure();
    server.join().expect("fixture server");

    assert!(output.join("turn-01.json").is_file());
    assert!(!output.join("turn-02.json").exists());
    let content = fs::read_to_string(output.join("turn-01.json")).expect("persisted first turn");
    assert!(!content.contains("run-contract-secret"));
}

#[test]
fn resume_b_reuses_two_persisted_turns_and_only_requests_the_revision() {
    let temp = tempfile::tempdir().expect("temp directory");
    let output = temp.path().join("run");
    fs::create_dir_all(&output).expect("run directory");
    fs::write(
        output.join("brief.json"),
        fs::read("corpus/corpus-v1.json").expect("corpus available"),
    )
    .expect("placeholder brief artifact");
    fs::write(output.join("turn-01.json"), prior_turn("skeleton")).expect("persist skeleton turn");
    fs::write(output.join("turn-02.json"), prior_turn(valid_spec()))
        .expect("persist arrangement turn");
    let revision = json!({
        "id": "revision-response",
        "model": "deepseek-v4-pro",
        "choices": [{"finish_reason": "stop", "message": {"content": valid_spec()}}],
        "usage": {"prompt_tokens": 600, "completion_tokens": 300, "total_tokens": 900}
    });
    let (base_url, server) = serve_responses(vec![revision]);

    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .env("DEEPSEEK_API_KEY", "run-contract-secret")
        .env("DEEPSEEK_BASE_URL", base_url)
        .env("DEEPSEEK_MODEL", "deepseek-v4-pro")
        .args([
            "resume-b",
            "--brief-id",
            "l1-song-hook",
            "--output-dir",
            output.to_str().expect("output path"),
        ])
        .assert()
        .success();
    server.join().expect("fixture server");

    assert!(output.join("turn-03.json").is_file());
    let run: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("run.json")).expect("run record"))
            .expect("run JSON");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["turn_count"], 3);
}

#[test]
fn resume_b_from_a_persisted_skeleton_does_not_repeat_the_first_turn() {
    let temp = tempfile::tempdir().expect("temp directory");
    let output = temp.path().join("run");
    fs::create_dir_all(&output).expect("run directory");
    fs::write(output.join("turn-01.json"), prior_turn("skeleton")).expect("persist skeleton turn");
    let arrangement = provider_response("arrangement-response", valid_spec());
    let revision = provider_response("revision-response", valid_spec());
    let (base_url, server) = serve_responses(vec![arrangement, revision]);

    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .env("DEEPSEEK_API_KEY", "run-contract-secret")
        .env("DEEPSEEK_BASE_URL", base_url)
        .env("DEEPSEEK_MODEL", "deepseek-v4-pro")
        .args([
            "resume-b",
            "--brief-id",
            "l1-song-hook",
            "--output-dir",
            output.to_str().expect("output path"),
        ])
        .assert()
        .success();
    server.join().expect("fixture server");

    assert!(output.join("turn-02.json").is_file());
    assert!(output.join("turn-03.json").is_file());
    let run: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("run.json")).expect("run record"))
            .expect("run JSON");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["turn_count"], 3);
}

#[test]
fn locked_mode_b_uses_one_audited_turn_for_global_resource_budget_repair() {
    let skeleton = provider_response(
        "repair-skeleton",
        r#"{"title":"skeleton","tempo_map":[],"key_map":[],"sections":[],"track_plan":[]}"#,
    );
    let arrangement = provider_response("repair-arrangement", valid_spec());
    let over_budget = resource_over_budget_spec();
    let revision = provider_response("repair-revision", &over_budget);
    let repaired = provider_response("repair-final", valid_spec());
    let (base_url, server) = serve_responses(vec![skeleton, arrangement, revision, repaired]);
    let temp = tempfile::tempdir().expect("temp directory");
    let evidence = temp.path().join("formal");
    let output = evidence.join("mode-b/l1-song-hook");
    let protocol = temp.path().join("protocol-v3.lock.json");
    fs::write(
        &protocol,
        r#"{
          "schema_version":"q0-protocol-v3-test",
          "run_binding_required":true,
          "provider":{"name":"deepseek","model_id":"deepseek-v4-pro","thinking_level":"high"},
          "modes":{"a_brief_ids":[],"b_brief_ids":["l1-song-hook"],"c_brief_ids":[]},
          "mode_b_resource_repair":{"max_turns":1},
          "gates":{"mode_b_valid_and_compiled_minimum":"1/1"}
        }"#,
    )
    .expect("protocol lock");

    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .env("DEEPSEEK_API_KEY", "run-contract-secret")
        .env("DEEPSEEK_BASE_URL", base_url)
        .env("DEEPSEEK_MODEL", "deepseek-v4-pro")
        .args([
            "run",
            "--mode",
            "b",
            "--brief-id",
            "l1-song-hook",
            "--output-dir",
            output.to_str().expect("output path"),
            "--protocol-lock",
            protocol.to_str().expect("protocol path"),
        ])
        .assert()
        .success();
    server.join().expect("fixture server");

    let run: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("run.json")).expect("run record"))
            .expect("run JSON");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["turn_count"], 4);
    assert!(output.join("turn-04.json").is_file());
    let binding: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("protocol-binding.json")).expect("protocol binding"),
    )
    .expect("binding JSON");
    assert_eq!(binding["protocol_id"], "q0-protocol-v3-test");
    assert_eq!(binding["mode_b_resource_repair_max_turns"], 1);
    assert_eq!(binding["mode_b_resource_repair_turns_used"], 1);

    let summary = temp.path().join("formal-summary.json");
    Command::cargo_bin("autostudio-music-quality")
        .expect("experiment binary")
        .args([
            "verify-formal",
            "--evidence-root",
            evidence.to_str().expect("evidence path"),
            "--output",
            summary.to_str().expect("summary path"),
            "--protocol-lock",
            protocol.to_str().expect("protocol path"),
        ])
        .assert()
        .success();
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(summary).expect("formal summary")).expect("summary JSON");
    assert_eq!(summary["mode_b_valid_and_compiled"], 1);
    assert_eq!(summary["mode_b_device_gate_passed"], true);
}

fn prior_turn(content: &str) -> Vec<u8> {
    serde_json::to_vec_pretty(&json!({
        "provider": "deepseek",
        "protocol": "openai-chat-completions",
        "model": "deepseek-v4-pro",
        "thinking_level": "high",
        "request": {},
        "response_id": "prior",
        "content": content,
        "finish_reason": "stop",
        "usage": {
            "prompt_tokens": 100,
            "prompt_cache_hit_tokens": 0,
            "prompt_cache_miss_tokens": 100,
            "completion_tokens": 100,
            "total_tokens": 200
        },
        "latency_ms": 10
    }))
    .expect("prior turn JSON")
}

fn provider_response(id: &str, content: &str) -> serde_json::Value {
    json!({
        "id": id,
        "model": "deepseek-v4-pro",
        "choices": [{"finish_reason": "stop", "message": {"content": content}}],
        "usage": {"prompt_tokens": 100, "completion_tokens": 100, "total_tokens": 200}
    })
}

fn resource_over_budget_spec() -> String {
    let mut value: serde_json::Value = serde_json::from_str(valid_spec()).expect("valid fixture");
    value["tracks"][0]["regions"][0]["cc"] = serde_json::Value::Array(
        (0..257)
            .map(|index| {
                json!({
                    "beat": f64::from(index % 32),
                    "controller": 11,
                    "value": 80
                })
            })
            .collect(),
    );
    serde_json::to_string(&value).expect("over-budget fixture")
}

fn serve_responses(responses: Vec<serde_json::Value>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let address = listener.local_addr().expect("fixture address");
    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = stream.read(&mut buffer).expect("read request");
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if request_complete(&bytes) {
                    break;
                }
            }
            let body = response.to_string();
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(reply.as_bytes()).expect("write response");
        }
    });
    (format!("http://{address}"), handle)
}

fn request_complete(bytes: &[u8]) -> bool {
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    bytes.len() >= header_end + 4 + content_length
}

fn valid_spec() -> &'static str {
    r#"{
      "title":"Open-window song hook",
      "tempo_map":[{"bar":1,"bpm":120,"time_signature":{"numerator":4,"denominator":4}}],
      "key_map":[{"bar":1,"tonic":"C","mode":"major"}],
      "sections":[{"id":"hook","label":"Hook","start_bar":1,"length_bars":8,"intent":"question and answer"}],
      "tracks":[
        {"id":"piano","name":"Piano","role":"harmony","register":{"low":48,"high":84},"instrument_hint":"piano","regions":[{"section_id":"hook","notes":[{"beat":0,"duration":4,"pitch":60,"velocity":80}],"cc":[]}]},
        {"id":"lead","name":"Lead","role":"melody","register":{"low":60,"high":88},"instrument_hint":"lead","regions":[{"section_id":"hook","notes":[{"beat":0,"duration":1,"pitch":72,"velocity":96}],"cc":[]}]},
        {"id":"bass","name":"Bass","role":"bass","register":{"low":28,"high":55},"instrument_hint":"bass","regions":[{"section_id":"hook","notes":[{"beat":0,"duration":2,"pitch":36,"velocity":88}],"cc":[]}]}
      ]
    }"#
}
