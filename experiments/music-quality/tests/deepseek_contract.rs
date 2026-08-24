use std::sync::mpsc;

use autostudio_music_quality::DeepSeekClient;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn deepseek_contract_sends_frozen_request_and_discards_private_reasoning() {
    let response = json!({
        "id": "q0-response-1",
        "model": "deepseek-v4-pro",
        "choices": [{
            "finish_reason": "stop",
            "message": {
                "content": "{\"title\":\"output\"}",
                "reasoning_content": "private-chain-sentinel"
            }
        }],
        "usage": {
            "prompt_tokens": 100,
            "prompt_cache_hit_tokens": 80,
            "prompt_cache_miss_tokens": 20,
            "completion_tokens": 50,
            "total_tokens": 150
        }
    });
    let (base_url, request) = serve_once(response).await;
    let client = DeepSeekClient::new(&base_url, "contract-secret", "deepseek-v4-pro", "high")
        .expect("client");

    let turn = client
        .generate_json("system", "user", 8_192)
        .await
        .expect("Provider turn");

    let (headers, body) = request.recv().expect("captured request");
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("authorization: bearer contract-secret")
    );
    assert_eq!(body["model"], "deepseek-v4-pro");
    assert_eq!(body["stream"], false);
    assert_eq!(body["response_format"]["type"], "json_object");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "high");
    assert_eq!(body["max_tokens"], 8_192);
    assert_eq!(turn.response_id.as_deref(), Some("q0-response-1"));
    assert_eq!(turn.usage.total_tokens, Some(150));
    assert_eq!(turn.usage.prompt_cache_hit_tokens, Some(80));
    assert_eq!(turn.usage.prompt_cache_miss_tokens, Some(20));
    assert_eq!(turn.content, "{\"title\":\"output\"}");
    assert!(
        !serde_json::to_string(&turn)
            .expect("serialized normalized turn")
            .contains("private-chain-sentinel")
    );
}

async fn serve_once(
    response: serde_json::Value,
) -> (String, mpsc::Receiver<(String, serde_json::Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("local address");
    let (sender, receiver) = mpsc::channel();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).await.expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = find_header_end(&bytes) {
                let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .expect("content length");
                let body_start = header_end + 4;
                if bytes.len() >= body_start + content_length {
                    let body =
                        serde_json::from_slice(&bytes[body_start..body_start + content_length])
                            .expect("request JSON");
                    sender.send((headers, body)).expect("capture request");
                    break;
                }
            }
        }
        let body = response.to_string();
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(reply.as_bytes()).await.expect("reply");
    });
    (format!("http://{address}"), receiver)
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}
