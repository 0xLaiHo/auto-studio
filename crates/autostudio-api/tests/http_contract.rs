use std::sync::Arc;

use autostudio_api::{router, router_with_connections};
use autostudio_core::project::{Project, ProjectService, ProjectStore, ProjectStoreError};
use autostudio_provider::connection::FileLlmConnectionManager;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn health_reports_the_ship_zero_protocol_version() {
    let app = router(
        Arc::new(ProjectService::new(Arc::new(EmptyProjectStore))),
        "test-session-token",
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/health")
                .body(Body::empty())
                .expect("health request"),
        )
        .await
        .expect("health response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("health body")
        .to_bytes();
    let health: serde_json::Value = serde_json::from_slice(&body).expect("health JSON");
    assert_eq!(health["status"], "ok");
    assert_eq!(health["coreVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(health["protocolVersion"], "0.3.0");
    assert_eq!(health["schemaVersion"], "1");
}

#[tokio::test]
async fn openapi_contract_is_public_and_describes_every_ship_zero_route() {
    let app = router(
        Arc::new(ProjectService::new(Arc::new(EmptyProjectStore))),
        "test-session-token",
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/openapi.json")
                .body(Body::empty())
                .expect("OpenAPI request"),
        )
        .await
        .expect("OpenAPI response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let body = response
        .into_body()
        .collect()
        .await
        .expect("OpenAPI body")
        .to_bytes();
    let contract: serde_json::Value = serde_json::from_slice(&body).expect("valid OpenAPI JSON");
    assert_eq!(contract["openapi"], "3.1.0");
    assert_eq!(contract["info"]["version"], "0.3.0");
    for path in [
        "/v1/health",
        "/v1/openapi.json",
        "/v1/projects",
        "/v1/projects/current",
        "/v1/projects/current/backup",
        "/v1/projects/current/brief",
        "/v1/projects/current/events",
        "/v1/provider-connections/llm",
        "/v1/provider-connections/llm/models",
        "/v1/providers/llm",
        "/v1/agent-runs",
        "/v1/agent-runs/{runId}/approval",
        "/v1/agent-runs/{runId}/execute",
        "/v1/agent-runs/{runId}/refresh",
        "/v1/agent-runs/{runId}/reconcile",
        "/v1/candidates/{candidateId}/selection",
        "/v1/assets/{assetVersionId}/preview",
        "/v1/handoffs",
    ] {
        assert!(contract["paths"].get(path).is_some(), "missing {path}");
    }
}

#[tokio::test]
async fn tui_can_configure_an_llm_connection_without_reading_the_key_back() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let connections = Arc::new(FileLlmConnectionManager::new(
        temp.path().join("config/llm-connection.json"),
        None,
    ));
    let app = router_with_connections(
        Arc::new(ProjectService::new(Arc::new(EmptyProjectStore))),
        "test-session-token",
        connections,
    );

    let configured = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/provider-connections/llm")
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "providerKind":"deepseek",
                      "model":"deepseek-chat",
                      "apiKey":"must-not-be-returned"
                    }"#,
                ))
                .expect("configure request"),
        )
        .await
        .expect("configure response");
    assert_eq!(configured.status(), StatusCode::OK);
    let configured = configured
        .into_body()
        .collect()
        .await
        .expect("configured body")
        .to_bytes();
    let configured_text = String::from_utf8(configured.to_vec()).expect("UTF-8 response");
    assert!(!configured_text.contains("must-not-be-returned"));
    let configured: serde_json::Value =
        serde_json::from_str(&configured_text).expect("configured JSON");
    assert_eq!(configured["configured"], true);
    assert_eq!(configured["source"], "private_file");

    let status = app
        .oneshot(
            Request::builder()
                .uri("/v1/provider-connections/llm")
                .header("authorization", "Bearer test-session-token")
                .body(Body::empty())
                .expect("status request"),
        )
        .await
        .expect("status response");
    let status = status
        .into_body()
        .collect()
        .await
        .expect("status body")
        .to_bytes();
    assert!(
        !status
            .as_ref()
            .windows(b"must-not-be-returned".len())
            .any(|part| { part == b"must-not-be-returned" })
    );
}

#[tokio::test]
async fn tui_can_fetch_and_select_models_after_connecting_a_provider() {
    let provider = axum::Router::new().route(
        "/models",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!({
                "object": "list",
                "data": [{"id": "deepseek-v4-flash"}, {"id": "deepseek-v4-pro"}]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("provider listener");
    let provider_url = format!(
        "http://{}",
        listener.local_addr().expect("provider address")
    );
    tokio::spawn(async move {
        axum::serve(listener, provider)
            .await
            .expect("provider server");
    });

    let temp = tempfile::tempdir().expect("temporary directory");
    let connections = Arc::new(FileLlmConnectionManager::new(
        temp.path().join("config/llm-connection.json"),
        None,
    ));
    let app = router_with_connections(
        Arc::new(ProjectService::new(Arc::new(EmptyProjectStore))),
        "test-session-token",
        connections,
    );

    let configured = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/provider-connections/llm")
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"providerKind":"deepseek","baseUrl":"{provider_url}","apiKey":"secret"}}"#
                )))
                .expect("configure request"),
        )
        .await
        .expect("configure response");
    assert_eq!(configured.status(), StatusCode::OK);

    let mut catalog = serde_json::Value::Null;
    for _ in 0..50 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/provider-connections/llm/models")
                    .header("authorization", "Bearer test-session-token")
                    .body(Body::empty())
                    .expect("catalog request"),
            )
            .await
            .expect("catalog response");
        let body = response
            .into_body()
            .collect()
            .await
            .expect("catalog body")
            .to_bytes();
        catalog = serde_json::from_slice(&body).expect("catalog JSON");
        if catalog["state"] == "ready" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(catalog["state"], "ready");
    assert_eq!(catalog["models"].as_array().map(Vec::len), Some(2));

    let selected = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/provider-connections/llm/models")
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"deepseek-v4-pro","modelEffort":"max"}"#,
                ))
                .expect("select model request"),
        )
        .await
        .expect("select model response");
    let body = selected
        .into_body()
        .collect()
        .await
        .expect("selected body")
        .to_bytes();
    let selected: serde_json::Value = serde_json::from_slice(&body).expect("selected JSON");
    assert_eq!(selected["model"], "deepseek-v4-pro");
    assert_eq!(selected["modelEffort"], "max");
}

#[tokio::test]
async fn creator_can_create_a_project_through_the_versioned_interface() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store =
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("night-drive.autostudio"))
            .expect("open project store");
    let app = router(
        Arc::new(ProjectService::new(Arc::new(store))),
        "test-session-token",
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("content-type", "application/json")
                .header("authorization", "Bearer test-session-token")
                .body(Body::from(r#"{"name":"Night Drive"}"#))
                .expect("create project request"),
        )
        .await
        .expect("create project response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("project body")
        .to_bytes();
    let project: serde_json::Value = serde_json::from_slice(&body).expect("project JSON");
    assert_eq!(project["name"], "Night Drive");
    assert_eq!(project["revision"], 0);
    Uuid::parse_str(project["id"].as_str().expect("project id")).expect("UUID project id");
}

#[tokio::test]
async fn project_commands_reject_requests_without_the_session_token() {
    let app = router(
        Arc::new(ProjectService::new(Arc::new(EmptyProjectStore))),
        "test-session-token",
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Night Drive"}"#))
                .expect("unauthorized request"),
        )
        .await
        .expect("unauthorized response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("unauthorized body")
        .to_bytes();
    assert_eq!(
        body.as_ref(),
        br#"{"code":"unauthorized","message":"a valid Core session token is required"}"#
    );
}

#[tokio::test]
async fn project_commands_reject_direct_browser_origin_requests() {
    let app = router(
        Arc::new(ProjectService::new(Arc::new(EmptyProjectStore))),
        "test-session-token",
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("authorization", "Bearer test-session-token")
                .header("origin", "http://localhost:1420")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Night Drive"}"#))
                .expect("browser-origin request"),
        )
        .await
        .expect("browser-origin response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("browser-origin body")
        .to_bytes();
    assert_eq!(
        body.as_ref(),
        br#"{"code":"browser_origin_forbidden","message":"browser origins must use the Desktop Core Client"}"#
    );
}

#[tokio::test]
async fn desktop_can_reopen_the_current_project_through_the_versioned_interface() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store =
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("night-drive.autostudio"))
            .expect("open project store");
    let app = router(
        Arc::new(ProjectService::new(Arc::new(store))),
        "test-session-token",
    );

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Night Drive"}"#))
                .expect("create project request"),
        )
        .await
        .expect("create project response")
        .into_body()
        .collect()
        .await
        .expect("created project body")
        .to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&created).expect("created project");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/projects/current")
                .header("authorization", "Bearer test-session-token")
                .body(Body::empty())
                .expect("reopen project request"),
        )
        .await
        .expect("reopen project response");

    assert_eq!(response.status(), StatusCode::OK);
    let reopened = response
        .into_body()
        .collect()
        .await
        .expect("reopened project body")
        .to_bytes();
    let reopened: serde_json::Value = serde_json::from_slice(&reopened).expect("reopened project");
    assert_eq!(reopened, created);
}

#[tokio::test]
async fn creator_can_save_a_brief_with_an_expected_project_revision() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = autostudio_storage::SqliteProjectStore::open(&temp.path().join("brief.autostudio"))
        .expect("open project store");
    let app = router(
        Arc::new(ProjectService::new(Arc::new(store))),
        "test-session-token",
    );
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Night Drive"}"#))
                .expect("create project request"),
        )
        .await
        .expect("create project response");

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/projects/current/brief")
                .header("authorization", "Bearer test-session-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "expectedRevision":0,
                      "brief":{
                        "summary":"A nocturnal synthwave cue",
                        "purpose":"short-film opening",
                        "style":["synthwave"],
                        "mood":["tense"],
                        "instrumentation":["analog synth"],
                        "targetDurationSeconds":90,
                        "lyrics":null,
                        "constraints":["instrumental"]
                      }
                    }"#,
                ))
                .expect("brief request"),
        )
        .await
        .expect("brief response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("brief body")
        .to_bytes();
    let project: serde_json::Value = serde_json::from_slice(&body).expect("project JSON");
    assert_eq!(project["revision"], 1);
    assert_eq!(project["brief"]["summary"], "A nocturnal synthwave cue");
}

#[tokio::test]
async fn desktop_can_resume_project_events_from_last_event_id() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store =
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("events.autostudio"))
            .expect("open project store");
    let projects = Arc::new(ProjectService::new(Arc::new(store)));
    projects
        .create_project("Night Drive")
        .expect("create project");
    projects
        .set_brief(
            0,
            autostudio_core::project::CreativeBriefDraft {
                summary: "Nocturnal synthwave".to_owned(),
                purpose: None,
                style: vec!["synthwave".to_owned()],
                mood: vec![],
                instrumentation: vec![],
                target_duration_seconds: Some(60),
                lyrics: None,
                constraints: vec![],
            },
        )
        .expect("set brief");
    let app = router(projects, "test-session-token");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/projects/current/events")
                .header("authorization", "Bearer test-session-token")
                .header("last-event-id", "1")
                .body(Body::empty())
                .expect("event stream request"),
        )
        .await
        .expect("event stream response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let frame = response
        .into_body()
        .frame()
        .await
        .expect("event frame")
        .expect("valid event frame");
    let payload = std::str::from_utf8(frame.data_ref().expect("event data")).expect("UTF-8 SSE");
    assert!(payload.contains("id: 2"));
    assert!(payload.contains("event: brief.updated"));
    assert!(!payload.contains("id: 1"));
}

struct EmptyProjectStore;

impl ProjectStore for EmptyProjectStore {
    fn create(&self, _project: &Project) -> Result<(), ProjectStoreError> {
        Err(ProjectStoreError::Unavailable(
            "not used by this contract".to_owned(),
        ))
    }

    fn open(&self) -> Result<Project, ProjectStoreError> {
        Err(ProjectStoreError::NotFound)
    }
}
