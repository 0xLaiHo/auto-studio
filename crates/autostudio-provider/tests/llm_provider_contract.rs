use std::sync::mpsc;

use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::ThinkingLevel;
use autostudio_provider::constants::{
    PROTOCOL_ANTHROPIC_MESSAGES, PROTOCOL_OPENAI_CHAT_COMPLETIONS, PROTOCOL_OPENAI_RESPONSES,
};
use autostudio_provider::llm::{HttpInferenceAdapter, LlmProtocol, LlmProviderConfig};
use autostudio_provider::{InferenceAdapter, InferenceRequest};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};

#[tokio::test]
async fn deepseek_chat_contract_sends_bearer_auth_and_parses_plan() {
    let response = json!({
        "id": "deepseek-response-1",
        "choices": [{"message": {"content": "{\"visibleSummary\":\"Two directions\",\"generationPrompt\":\"warm acoustic ensemble\",\"durationSeconds\":30,\"candidateCount\":2}"}}],
        "usage": {"prompt_tokens": 25, "completion_tokens": 12}
    });
    let (base_url, request) = serve_once("/chat/completions", response).await;
    let adapter = adapter(
        "deepseek",
        LlmProtocol::OpenAiChatCompletions,
        &base_url,
        "deepseek-v4-flash",
    );

    let outcome = adapter
        .infer(inference_request())
        .await
        .expect("DeepSeek-compatible response");
    let (headers, body) = request.recv().expect("captured request");

    assert_eq!(headers.get("authorization").unwrap(), "Bearer test-secret");
    assert_eq!(body["response_format"]["type"], "json_object");
    assert_eq!(body["max_tokens"], 4_096);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "high");
    assert!(!body.to_string().contains("test-secret"));
    assert_eq!(
        adapter.descriptor().protocol,
        PROTOCOL_OPENAI_CHAT_COMPLETIONS
    );
    assert_eq!(outcome.response_id.as_deref(), Some("deepseek-response-1"));
    assert_eq!(outcome.usage.input_tokens, Some(25));
}

#[tokio::test]
async fn openai_responses_contract_uses_strict_schema_and_parses_nested_text() {
    let response = json!({
        "id": "resp_123",
        "output": [{"content": [{
            "type": "output_text",
            "text": "{\"visibleSummary\":\"One direction\",\"generationPrompt\":\"dry studio drums\",\"durationSeconds\":20,\"candidateCount\":1}"
        }]}],
        "usage": {"input_tokens": 18, "output_tokens": 9}
    });
    let (base_url, request) = serve_once("/responses", response).await;
    let adapter = adapter("openai", LlmProtocol::OpenAiResponses, &base_url, "gpt-5.2");

    let outcome = adapter
        .infer(inference_request())
        .await
        .expect("OpenAI Responses response");
    let (headers, body) = request.recv().expect("captured request");

    assert_eq!(headers.get("authorization").unwrap(), "Bearer test-secret");
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["strict"], true);
    assert_eq!(body["store"], false);
    assert_eq!(body["max_output_tokens"], 4_096);
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["reasoning"]["summary"], "auto");
    assert_eq!(body["include"][0], "reasoning.encrypted_content");
    assert_eq!(adapter.descriptor().protocol, PROTOCOL_OPENAI_RESPONSES);
    assert_eq!(outcome.usage.output_tokens, Some(9));
}

#[tokio::test]
async fn anthropic_messages_contract_forces_the_typed_plan_tool() {
    let response = json!({
        "id": "msg_123",
        "content": [{
            "type": "tool_use",
            "name": "submit_creative_plan",
            "input": {
                "visibleSummary": "Four sketches",
                "generationPrompt": "cinematic strings",
                "durationSeconds": 45,
                "candidateCount": 4
            }
        }],
        "usage": {"input_tokens": 30, "output_tokens": 15}
    });
    let (base_url, request) = serve_once("/v1/messages", response).await;
    let adapter = adapter(
        "anthropic",
        LlmProtocol::AnthropicMessages,
        &base_url,
        "claude-sonnet-4-6",
    );

    let outcome = adapter
        .infer(inference_request())
        .await
        .expect("Anthropic Messages response");
    let (headers, body) = request.recv().expect("captured request");

    assert_eq!(headers.get("x-api-key").unwrap(), "test-secret");
    assert_eq!(headers.get("anthropic-version").unwrap(), "2023-06-01");
    assert_eq!(body["tool_choice"]["name"], "submit_creative_plan");
    assert_eq!(body["max_tokens"], 4_096);
    assert_eq!(body["thinking"]["type"], "adaptive");
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(adapter.descriptor().protocol, PROTOCOL_ANTHROPIC_MESSAGES);
    assert_eq!(outcome.response_id.as_deref(), Some("msg_123"));
}

#[tokio::test]
async fn anthropic_manual_thinking_uses_auto_tool_choice_and_reserves_answer_tokens() {
    let response = json!({
        "id": "msg_legacy",
        "content": [{
            "type": "tool_use",
            "name": "submit_creative_plan",
            "input": {
                "visibleSummary": "Legacy thinking plan",
                "generationPrompt": "acoustic pulse",
                "durationSeconds": 30,
                "candidateCount": 1
            }
        }]
    });
    let (base_url, request) = serve_once("/v1/messages", response).await;
    let adapter = HttpInferenceAdapter::new(
        LlmProviderConfig::new(
            "anthropic",
            LlmProtocol::AnthropicMessages,
            &base_url,
            "claude-sonnet-4-5",
            "test-secret",
        )
        .expect("test Provider config")
        .with_thinking_level(ThinkingLevel::Max),
    )
    .expect("test Provider adapter");

    adapter
        .infer(inference_request())
        .await
        .expect("Anthropic legacy response");
    let (_, body) = request.recv().expect("captured request");
    assert_eq!(body["thinking"]["budget_tokens"], 16_384);
    assert_eq!(body["max_tokens"], 20_480);
    assert_eq!(body["tool_choice"]["type"], "auto");
}

#[tokio::test]
async fn kimi_open_k3_sends_only_its_supported_effort_contract() {
    let response = json!({
        "id": "kimi-open-response",
        "choices": [{"message": {"content": "{\"visibleSummary\":\"Kimi direction\",\"generationPrompt\":\"bright chamber pop\",\"durationSeconds\":30,\"candidateCount\":1}"}}]
    });
    let (base_url, request) = serve_once("/chat/completions", response).await;
    let adapter = adapter(
        "kimi-open",
        LlmProtocol::OpenAiChatCompletions,
        &base_url,
        "kimi-k3",
    );

    adapter
        .infer(inference_request())
        .await
        .expect("Kimi Open response");
    let (_, body) = request.recv().expect("captured request");
    assert_eq!(body["reasoning_effort"], "high");
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn kimi_code_k3_uses_anthropic_adaptive_thinking_contract() {
    let response = json!({
        "id": "kimi-code-response",
        "content": [{
            "type": "tool_use",
            "name": "submit_creative_plan",
            "input": {
                "visibleSummary": "Kimi Code direction",
                "generationPrompt": "minimal electronic pulse",
                "durationSeconds": 30,
                "candidateCount": 1
            }
        }]
    });
    let (base_url, request) = serve_once("/v1/messages", response).await;
    let adapter = adapter(
        "kimi-code",
        LlmProtocol::AnthropicMessages,
        &base_url,
        "k3-256k",
    );

    adapter
        .infer(inference_request())
        .await
        .expect("Kimi Code response");
    let (_, body) = request.recv().expect("captured request");
    assert_eq!(body["thinking"]["type"], "adaptive");
    assert_eq!(body["thinking"]["display"], "summarized");
    assert_eq!(body["output_config"]["effort"], "high");
}

#[tokio::test]
async fn deepseek_max_effort_is_sent_to_the_real_request_contract() {
    let response = json!({
        "id": "deepseek-response-max",
        "choices": [{"message": {"content": "{\"visibleSummary\":\"One direction\",\"generationPrompt\":\"cinematic piano\",\"durationSeconds\":30,\"candidateCount\":1}"}}]
    });
    let (base_url, request) = serve_once("/chat/completions", response).await;
    let adapter = HttpInferenceAdapter::new(
        LlmProviderConfig::new(
            "deepseek",
            LlmProtocol::OpenAiChatCompletions,
            &base_url,
            "deepseek-v4-pro",
            "test-secret",
        )
        .expect("test Provider config")
        .with_thinking_level(ThinkingLevel::Max),
    )
    .expect("test Provider adapter");

    adapter
        .infer(inference_request())
        .await
        .expect("DeepSeek max-effort response");
    let (_, body) = request.recv().expect("captured request");
    assert_eq!(body["max_tokens"], 4_096);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "max");
}

fn adapter(
    provider: &str,
    protocol: LlmProtocol,
    base_url: &str,
    model: &str,
) -> HttpInferenceAdapter {
    HttpInferenceAdapter::new(
        LlmProviderConfig::new(provider, protocol, base_url, model, "test-secret")
            .expect("test Provider config")
            .with_thinking_level(ThinkingLevel::High),
    )
    .expect("test Provider adapter")
}

fn inference_request() -> InferenceRequest {
    let temp = tempfile::tempdir().expect("temporary project");
    let store = autostudio_storage::SqliteProjectStore::open(&temp.path().join("brief.autostudio"))
        .expect("project store");
    let projects = ProjectService::new(std::sync::Arc::new(store));
    projects
        .create_project("Provider contract")
        .expect("project");
    let project = projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A polished acoustic cue".to_owned(),
                purpose: Some("opening titles".to_owned()),
                style: vec!["acoustic".to_owned()],
                mood: vec!["warm".to_owned()],
                instrumentation: vec!["piano".to_owned()],
                target_duration_seconds: Some(30),
                lyrics: None,
                constraints: vec!["instrumental".to_owned()],
            },
        )
        .expect("brief");
    InferenceRequest {
        brief: project.brief().expect("saved brief").clone(),
        context_revision: project.revision(),
    }
}

async fn serve_once(
    path: &'static str,
    response: Value,
) -> (String, mpsc::Receiver<(HeaderMap, Value)>) {
    let (sender, receiver) = mpsc::channel();
    let app = Router::new().route(
        path,
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let sender = sender.clone();
            let response = response.clone();
            async move {
                sender.send((headers, body)).expect("capture request");
                Json(response)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener");
    let address = listener.local_addr().expect("test address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test server");
    });
    (format!("http://{address}"), receiver)
}
