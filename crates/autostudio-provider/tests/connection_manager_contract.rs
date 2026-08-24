use std::sync::{Arc, mpsc};

use autostudio_core::project::{CreativeBriefDraft, ProjectService};
use autostudio_core::provider::{
    LlmConnectionConfiguration, LlmConnectionControl, LlmConnectionSource, LlmModelCatalogState,
    ThinkingLevel,
};
use autostudio_provider::connection::{ConnectionInferenceAdapter, FileLlmConnectionManager};
use autostudio_provider::{InferenceAdapter, InferenceRequest};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

#[tokio::test]
async fn a_private_connection_is_used_by_the_next_real_inference_request() {
    let response = json!({
        "id": "configured-response-1",
        "choices": [{"message": {"content": "{\"visibleSummary\":\"Configured direction\",\"generationPrompt\":\"warm acoustic ensemble\",\"durationSeconds\":30,\"candidateCount\":2}"}}],
        "usage": {"prompt_tokens": 17, "completion_tokens": 8}
    });
    let (base_url, request) = serve_once("/chat/completions", response).await;
    let temp = tempfile::tempdir().expect("temporary connection home");
    let connection_path = temp.path().join("config/llm-connection.json");
    let manager = Arc::new(FileLlmConnectionManager::new(&connection_path, None));

    assert!(!manager.status().expect("initial status").configured);
    let configured = manager
        .configure(LlmConnectionConfiguration::new(
            "deepseek",
            Some("deepseek-contract-model".to_owned()),
            Some(base_url),
            "private-test-secret",
        ))
        .expect("configure private connection");
    assert!(configured.configured);
    assert_eq!(configured.source, Some(LlmConnectionSource::PrivateFile));
    assert_eq!(configured.provider_kind.as_deref(), Some("deepseek"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&connection_path)
            .expect("connection metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "credential file must not be shared");
    }

    let adapter = ConnectionInferenceAdapter::new(manager);
    let outcome = adapter
        .infer(inference_request())
        .await
        .expect("inference through configured connection");
    let (headers, body) = request.recv().expect("captured request");
    assert_eq!(
        headers.get("authorization").expect("authorization"),
        "Bearer private-test-secret"
    );
    assert!(!body.to_string().contains("private-test-secret"));
    assert_eq!(outcome.provider.provider_kind, "deepseek");
    assert_eq!(outcome.provider.model, "deepseek-contract-model");
    assert_eq!(
        outcome.response_id.as_deref(),
        Some("configured-response-1")
    );
}

#[tokio::test]
async fn a_connection_fetches_and_persists_the_selectable_model_catalog() {
    let (base_url, request) = serve_model_catalog(json!({
        "object": "list",
        "data": [
            {"id": "deepseek-v4-pro"},
            {"id": "deepseek-v4-flash"}
        ]
    }))
    .await;
    let temp = tempfile::tempdir().expect("temporary connection home");
    let connection_path = temp.path().join("config/llm-connection.json");
    let manager = FileLlmConnectionManager::new(&connection_path, None);

    let configured = manager
        .configure(LlmConnectionConfiguration::new(
            "deepseek",
            None,
            Some(base_url),
            "catalog-test-secret",
        ))
        .expect("configure connection");
    assert_eq!(configured.model, None);
    assert_eq!(configured.catalog.state, LlmModelCatalogState::Refreshing);

    let catalog = manager
        .refresh_model_catalog()
        .await
        .expect("refresh model catalog");
    let headers = request.recv().expect("captured catalog request");
    assert_eq!(
        headers.get("authorization").expect("authorization"),
        "Bearer catalog-test-secret"
    );
    assert_eq!(catalog.state, LlmModelCatalogState::Ready);
    assert_eq!(catalog.models[0].id, "deepseek-v4-flash");
    assert_eq!(catalog.models[1].id, "deepseek-v4-pro");
    let invalid = manager
        .select_model("deepseek-v4-pro", ThinkingLevel::Medium)
        .expect_err("unsupported model level must be rejected before inference");
    assert!(matches!(
        invalid,
        autostudio_core::provider::ProviderConnectionError::ThinkingLevelNotAvailable { .. }
    ));

    let selected = manager
        .select_model("deepseek-v4-pro", ThinkingLevel::Max)
        .expect("select catalog model");
    assert_eq!(selected.model.as_deref(), Some("deepseek-v4-pro"));
    assert_eq!(selected.thinking_level, ThinkingLevel::Max);
    assert_eq!(
        selected.model_thinking_levels.get("deepseek-v4-pro"),
        Some(&ThinkingLevel::Max)
    );

    let reopened = FileLlmConnectionManager::new(&connection_path, None)
        .status()
        .expect("reopen connection status");
    assert_eq!(reopened.model.as_deref(), Some("deepseek-v4-pro"));
    assert_eq!(reopened.thinking_level, ThinkingLevel::Max);
    assert_eq!(reopened.catalog.models, catalog.models);
}

#[test]
fn schema_three_connections_are_read_with_model_capabilities_and_preferences() {
    let temp = tempfile::tempdir().expect("temporary connection home");
    let connection_path = temp.path().join("config/llm-connection.json");
    std::fs::create_dir_all(connection_path.parent().expect("connection parent"))
        .expect("create connection parent");
    std::fs::write(
        &connection_path,
        serde_json::to_vec(&json!({
            "schemaVersion": "autostudio.llm-connection/3",
            "connectionId": "49de5ee6-3b9f-4b35-a666-b2f451b392ca",
            "providerKind": "deepseek",
            "model": "deepseek-v4-pro",
            "modelEffort": "max",
            "baseUrl": "https://api.deepseek.com",
            "apiKey": "legacy-private-secret",
            "catalog": {
                "state": "ready",
                "models": [{
                    "id": "deepseek-v4-pro",
                    "displayName": "DeepSeek V4 Pro"
                }],
                "error": null
            }
        }))
        .expect("serialize legacy connection"),
    )
    .expect("write legacy connection");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&connection_path, std::fs::Permissions::from_mode(0o600))
            .expect("make legacy connection private");
    }

    let status = FileLlmConnectionManager::new(&connection_path, None)
        .status()
        .expect("read schema three connection");

    assert_eq!(status.thinking_level, ThinkingLevel::Max);
    assert_eq!(
        status.model_thinking_levels.get("deepseek-v4-pro"),
        Some(&ThinkingLevel::Max)
    );
    assert_eq!(
        status.catalog.models[0].thinking.levels,
        vec![ThinkingLevel::Off, ThinkingLevel::High, ThinkingLevel::Max]
    );
}

#[test]
fn current_connection_cache_cannot_enable_an_unsupported_thinking_level() {
    let temp = tempfile::tempdir().expect("temporary connection home");
    let connection_path = temp.path().join("config/llm-connection.json");
    std::fs::create_dir_all(connection_path.parent().expect("connection parent"))
        .expect("create connection parent");
    std::fs::write(
        &connection_path,
        serde_json::to_vec(&json!({
            "schemaVersion": "autostudio.llm-connection/4",
            "connectionId": "79ce40db-1050-427a-a8fb-4356ba2af0b5",
            "providerKind": "deepseek",
            "model": "deepseek-v4-pro",
            "modelEffort": "medium",
            "modelThinkingLevels": {"deepseek-v4-pro": "medium"},
            "baseUrl": "https://api.deepseek.com",
            "apiKey": "private-secret",
            "catalog": {
                "state": "ready",
                "models": [{
                    "id": "deepseek-v4-pro",
                    "displayName": "DeepSeek V4 Pro",
                    "thinking": {
                        "control": "token_budget",
                        "levels": ["medium"],
                        "defaultLevel": "medium"
                    }
                }],
                "error": null
            }
        }))
        .expect("serialize connection"),
    )
    .expect("write connection");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&connection_path, std::fs::Permissions::from_mode(0o600))
            .expect("make connection private");
    }

    let status = FileLlmConnectionManager::new(&connection_path, None)
        .status()
        .expect("read current connection");

    assert_eq!(status.thinking_level, ThinkingLevel::High);
    assert_eq!(
        status.model_thinking_levels.get("deepseek-v4-pro"),
        Some(&ThinkingLevel::High)
    );
    assert_eq!(
        status.catalog.models[0].thinking.levels,
        vec![ThinkingLevel::Off, ThinkingLevel::High, ThinkingLevel::Max]
    );
}

fn inference_request() -> InferenceRequest {
    let temp = tempfile::tempdir().expect("temporary project");
    let store = autostudio_storage::SqliteProjectStore::open(&temp.path().join("brief.autostudio"))
        .expect("project store");
    let projects = ProjectService::new(Arc::new(store));
    projects
        .create_project("Connection contract")
        .expect("project");
    let project = projects
        .set_brief(
            0,
            CreativeBriefDraft {
                summary: "A polished acoustic cue".to_owned(),
                purpose: None,
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

async fn serve_model_catalog(response: Value) -> (String, mpsc::Receiver<HeaderMap>) {
    let (sender, receiver) = mpsc::channel();
    let app = Router::new().route(
        "/models",
        get(move |headers: HeaderMap| {
            let sender = sender.clone();
            let response = response.clone();
            async move {
                sender.send(headers).expect("capture request");
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
