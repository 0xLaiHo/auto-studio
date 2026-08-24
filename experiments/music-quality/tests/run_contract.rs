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
