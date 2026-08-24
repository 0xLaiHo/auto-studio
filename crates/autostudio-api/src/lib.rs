//! Versioned loopback HTTP and SSE interface.

mod constants;
pub mod discovery;
mod error;

use std::convert::Infallible;
use std::sync::Arc;

use autostudio_core::agent::{AgentRunId, CostApproval};
use autostudio_core::production::{
    AssetVersionId, CandidateId, HandoffSink, PreviewByteRange, PreviewSource,
};
use autostudio_core::project::{
    CreativeBriefDraft, Project, ProjectBackup, ProjectBackupSink, ProjectService,
};
use autostudio_core::provider::{
    LlmConnectionConfiguration, LlmConnectionControl, LlmConnectionStatus, LlmModelCatalog,
    LlmProviderDescriptor, ThinkingLevel,
};
use autostudio_core::runtime::CreativeRuntime;
use axum::body::Body;
use axum::extract::{Path as AxumPath, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{
    Json, Router,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::constants::{EVENT_POLL_INTERVAL, OPENAPI_V1};
use crate::error::ApiError;

pub use crate::constants::{CORE_VERSION, PROTOCOL_VERSION, SCHEMA_VERSION};

pub fn router(projects: Arc<ProjectService>, session_token: &str) -> Router {
    build_router(projects, session_token, None, None, None, None, None)
}

pub fn router_with_connections(
    projects: Arc<ProjectService>,
    session_token: &str,
    llm_connections: Arc<dyn LlmConnectionControl>,
) -> Router {
    build_router(
        projects,
        session_token,
        None,
        None,
        None,
        None,
        Some(llm_connections),
    )
}

pub fn router_with_runtime(
    projects: Arc<ProjectService>,
    session_token: &str,
    runtime: Arc<dyn CreativeRuntime>,
) -> Router {
    build_router(
        projects,
        session_token,
        Some(runtime),
        None,
        None,
        None,
        None,
    )
}

pub fn router_with_runtime_and_handoff(
    projects: Arc<ProjectService>,
    session_token: &str,
    runtime: Arc<dyn CreativeRuntime>,
    handoff: Arc<dyn HandoffSink>,
) -> Router {
    build_router(
        projects,
        session_token,
        Some(runtime),
        Some(handoff),
        None,
        None,
        None,
    )
}

pub fn router_with_runtime_and_media(
    projects: Arc<ProjectService>,
    session_token: &str,
    runtime: Arc<dyn CreativeRuntime>,
    handoff: Arc<dyn HandoffSink>,
    preview: Arc<dyn PreviewSource>,
) -> Router {
    build_router(
        projects,
        session_token,
        Some(runtime),
        Some(handoff),
        Some(preview),
        None,
        None,
    )
}

pub fn router_with_runtime_media_and_backup(
    projects: Arc<ProjectService>,
    session_token: &str,
    runtime: Arc<dyn CreativeRuntime>,
    handoff: Arc<dyn HandoffSink>,
    preview: Arc<dyn PreviewSource>,
    backup: Arc<dyn ProjectBackupSink>,
) -> Router {
    build_router(
        projects,
        session_token,
        Some(runtime),
        Some(handoff),
        Some(preview),
        Some(backup),
        None,
    )
}

pub fn router_with_runtime_media_backup_and_connections(
    projects: Arc<ProjectService>,
    session_token: &str,
    runtime: Arc<dyn CreativeRuntime>,
    handoff: Arc<dyn HandoffSink>,
    preview: Arc<dyn PreviewSource>,
    backup: Arc<dyn ProjectBackupSink>,
    llm_connections: Arc<dyn LlmConnectionControl>,
) -> Router {
    build_router(
        projects,
        session_token,
        Some(runtime),
        Some(handoff),
        Some(preview),
        Some(backup),
        Some(llm_connections),
    )
}

fn build_router(
    projects: Arc<ProjectService>,
    session_token: &str,
    runtime: Option<Arc<dyn CreativeRuntime>>,
    handoff: Option<Arc<dyn HandoffSink>>,
    preview: Option<Arc<dyn PreviewSource>>,
    backup: Option<Arc<dyn ProjectBackupSink>>,
    llm_connections: Option<Arc<dyn LlmConnectionControl>>,
) -> Router {
    let protected = Router::new()
        .route("/v1/projects", post(create_project))
        .route("/v1/projects/current", get(open_project))
        .route("/v1/projects/current/backup", post(backup_project))
        .route("/v1/projects/current/brief", put(set_brief))
        .route("/v1/projects/current/events", get(project_events))
        .route(
            "/v1/provider-connections/llm",
            get(llm_connection_status).put(configure_llm_connection),
        )
        .route("/v1/providers/llm", get(llm_providers))
        .route(
            "/v1/provider-connections/llm/models",
            get(llm_model_catalog)
                .post(refresh_llm_model_catalog)
                .put(select_llm_model),
        )
        .route("/v1/agent-runs", post(plan_agent_run))
        .route("/v1/agent-runs/{run_id}/approval", post(approve_agent_run))
        .route("/v1/agent-runs/{run_id}/execute", post(execute_agent_run))
        .route(
            "/v1/agent-runs/{run_id}/reconcile",
            post(reconcile_agent_run),
        )
        .route("/v1/agent-runs/{run_id}/refresh", post(refresh_agent_run))
        .route(
            "/v1/candidates/{candidate_id}/selection",
            post(select_candidate),
        )
        .route("/v1/handoffs", post(export_handoff))
        .route("/v1/assets/{asset_version_id}/preview", get(preview_asset))
        .route_layer(middleware::from_fn_with_state(
            Arc::<str>::from(session_token),
            require_session,
        ));

    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/openapi.json", get(openapi))
        .merge(protected)
        .with_state(ApiState {
            projects,
            runtime,
            handoff,
            preview,
            backup,
            llm_connections,
        })
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        core_version: CORE_VERSION,
        protocol_version: PROTOCOL_VERSION,
        schema_version: SCHEMA_VERSION,
    })
}

async fn openapi() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], OPENAPI_V1)
}

async fn create_project(
    State(state): State<ApiState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let project = state.projects.create_project(&request.name)?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn open_project(State(state): State<ApiState>) -> Result<Json<Project>, ApiError> {
    Ok(Json(state.projects.open_project()?))
}

async fn backup_project(
    State(state): State<ApiState>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Result<Json<ProjectBackup>, ApiError> {
    let backup = state.backup.as_ref().ok_or_else(backup_unavailable)?;
    Ok(Json(state.projects.backup_project(
        request.expected_revision,
        backup.as_ref(),
    )?))
}

async fn set_brief(
    State(state): State<ApiState>,
    Json(request): Json<SetBriefRequest>,
) -> Result<Json<Project>, ApiError> {
    Ok(Json(
        state
            .projects
            .set_brief(request.expected_revision, request.brief)?,
    ))
}

async fn llm_connection_status(
    State(state): State<ApiState>,
) -> Result<Json<LlmConnectionStatus>, ApiError> {
    let connections = state
        .llm_connections
        .as_ref()
        .ok_or_else(llm_connections_unavailable)?;
    Ok(Json(connections.status()?))
}

async fn llm_providers(
    State(state): State<ApiState>,
) -> Result<Json<Vec<LlmProviderDescriptor>>, ApiError> {
    let connections = state
        .llm_connections
        .as_ref()
        .ok_or_else(llm_connections_unavailable)?;
    Ok(Json(connections.providers()))
}

async fn llm_model_catalog(
    State(state): State<ApiState>,
) -> Result<Json<LlmModelCatalog>, ApiError> {
    let connections = state
        .llm_connections
        .as_ref()
        .ok_or_else(llm_connections_unavailable)?;
    Ok(Json(connections.model_catalog()?))
}

async fn refresh_llm_model_catalog(
    State(state): State<ApiState>,
) -> Result<Json<LlmModelCatalog>, ApiError> {
    let connections = state
        .llm_connections
        .as_ref()
        .ok_or_else(llm_connections_unavailable)?;
    Ok(Json(connections.refresh_model_catalog().await?))
}

async fn select_llm_model(
    State(state): State<ApiState>,
    Json(request): Json<SelectLlmModelRequest>,
) -> Result<Json<LlmConnectionStatus>, ApiError> {
    let connections = state
        .llm_connections
        .as_ref()
        .ok_or_else(llm_connections_unavailable)?;
    Ok(Json(
        connections.select_model(&request.model, request.thinking_level)?,
    ))
}

async fn configure_llm_connection(
    State(state): State<ApiState>,
    Json(request): Json<ConfigureLlmConnectionRequest>,
) -> Result<Json<LlmConnectionStatus>, ApiError> {
    let connections = state
        .llm_connections
        .as_ref()
        .ok_or_else(llm_connections_unavailable)?;
    let status = connections.configure(LlmConnectionConfiguration::new(
        request.provider_kind,
        request.model,
        request.base_url,
        request.api_key,
    ))?;
    let connections = Arc::clone(connections);
    tokio::spawn(async move {
        let _ = connections.refresh_model_catalog().await;
    });
    Ok(Json(status))
}

async fn project_events(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let after_sequence = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let stream = async_stream::stream! {
        let mut cursor = after_sequence;
        loop {
            match state.projects.events_after(cursor) {
                Ok(events) => {
                    for envelope in events {
                        cursor = envelope.sequence();
                        let event = Event::default()
                            .id(cursor.to_string())
                            .event(envelope.event().kind_name())
                            .json_data(&envelope)
                            .unwrap_or_else(|_| Event::default().event("project.stream_error"));
                        yield Ok::<Event, Infallible>(event);
                    }
                }
                Err(error) => {
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .event("project.stream_error")
                            .data(error.to_string()),
                    );
                    break;
                }
            }
            tokio::time::sleep(EVENT_POLL_INTERVAL).await;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn plan_agent_run(
    State(state): State<ApiState>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Result<Json<Project>, ApiError> {
    let runtime = state.runtime.as_ref().ok_or_else(runtime_unavailable)?;
    Ok(Json(runtime.plan(request.expected_revision).await?))
}

async fn approve_agent_run(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<ApproveAgentRunRequest>,
) -> Result<Json<Project>, ApiError> {
    let run_id = AgentRunId::parse(&run_id).map_err(|error| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_agent_run_id",
        message: error.to_string(),
    })?;
    Ok(Json(state.projects.approve_agent_run(
        request.expected_revision,
        &run_id,
        request.approval,
    )?))
}

async fn execute_agent_run(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Result<Json<Project>, ApiError> {
    let runtime = state.runtime.as_ref().ok_or_else(runtime_unavailable)?;
    let run_id = AgentRunId::parse(&run_id).map_err(|error| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_agent_run_id",
        message: error.to_string(),
    })?;
    Ok(Json(
        runtime
            .execute_approved(request.expected_revision, run_id)
            .await?,
    ))
}

async fn reconcile_agent_run(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Result<Json<Project>, ApiError> {
    let runtime = state.runtime.as_ref().ok_or_else(runtime_unavailable)?;
    let run_id = AgentRunId::parse(&run_id).map_err(|error| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_agent_run_id",
        message: error.to_string(),
    })?;
    Ok(Json(
        runtime
            .reconcile_unknown(request.expected_revision, run_id)
            .await?,
    ))
}

async fn refresh_agent_run(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Result<Json<Project>, ApiError> {
    let runtime = state.runtime.as_ref().ok_or_else(runtime_unavailable)?;
    let run_id = AgentRunId::parse(&run_id).map_err(|error| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_agent_run_id",
        message: error.to_string(),
    })?;
    Ok(Json(
        runtime
            .resume_submitted(request.expected_revision, run_id)
            .await?,
    ))
}

async fn select_candidate(
    State(state): State<ApiState>,
    AxumPath(candidate_id): AxumPath<String>,
    Json(request): Json<SelectCandidateRequest>,
) -> Result<Json<Project>, ApiError> {
    let candidate_id = CandidateId::parse(&candidate_id).map_err(|error| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_candidate_id",
        message: error.to_string(),
    })?;
    Ok(Json(state.projects.select_candidate(
        request.expected_revision,
        &candidate_id,
        request.start_micros,
    )?))
}

async fn export_handoff(
    State(state): State<ApiState>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Result<Json<Project>, ApiError> {
    let handoff = state.handoff.as_ref().ok_or_else(handoff_unavailable)?;
    Ok(Json(state.projects.export_handoff(
        request.expected_revision,
        handoff.as_ref(),
    )?))
}

async fn preview_asset(
    State(state): State<ApiState>,
    AxumPath(asset_version_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let preview = state.preview.as_ref().ok_or_else(preview_unavailable)?;
    let asset_version_id = AssetVersionId::parse(&asset_version_id).map_err(|error| ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_asset_version_id",
        message: error.to_string(),
    })?;
    let range = parse_preview_range(headers.get(header::RANGE))?;
    let asset = state.projects.preview_asset(&asset_version_id)?;
    let chunk = preview.read(&asset, range).map_err(|_| ApiError {
        status: StatusCode::RANGE_NOT_SATISFIABLE,
        code: "preview_unavailable",
        message: "Audio preview is unavailable or the byte range is invalid".to_owned(),
    })?;
    let partial = range.is_some();
    let mut response = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, chunk.media_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, chunk.bytes.len().to_string());
    if partial {
        response = response.header(
            header::CONTENT_RANGE,
            format!(
                "bytes {}-{}/{}",
                chunk.start, chunk.end_inclusive, chunk.total_size
            ),
        );
    }
    response
        .body(Body::from(chunk.bytes))
        .map_err(|_| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "preview_response_failed",
            message: "Audio preview response could not be built".to_owned(),
        })
}

fn parse_preview_range(
    value: Option<&axum::http::HeaderValue>,
) -> Result<Option<PreviewByteRange>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| invalid_preview_range())?;
    let (unit, value) = value.split_once('=').ok_or_else(invalid_preview_range)?;
    let (start, end) = value.split_once('-').ok_or_else(invalid_preview_range)?;
    if unit != "bytes" || start.is_empty() || value.contains(',') {
        return Err(invalid_preview_range());
    }
    Ok(Some(PreviewByteRange {
        start: start.parse().map_err(|_| invalid_preview_range())?,
        end_inclusive: if end.is_empty() {
            None
        } else {
            Some(end.parse().map_err(|_| invalid_preview_range())?)
        },
    }))
}

fn invalid_preview_range() -> ApiError {
    ApiError {
        status: StatusCode::RANGE_NOT_SATISFIABLE,
        code: "invalid_preview_range",
        message: "Preview supports one explicit bytes=start-end range".to_owned(),
    }
}

fn runtime_unavailable() -> ApiError {
    ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "creative_runtime_unavailable",
        message: "Creative Agent runtime is not configured".to_owned(),
    }
}

fn handoff_unavailable() -> ApiError {
    ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "handoff_unavailable",
        message: "DAW Handoff is not configured".to_owned(),
    }
}

fn preview_unavailable() -> ApiError {
    ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "preview_unavailable",
        message: "Preview Playback is not configured".to_owned(),
    }
}

fn backup_unavailable() -> ApiError {
    ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "project_backup_unavailable",
        message: "Project backup is not configured".to_owned(),
    }
}

fn llm_connections_unavailable() -> ApiError {
    ApiError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "llm_connections_unavailable",
        message: "LLM Provider Connection management is not configured".to_owned(),
    }
}

async fn require_session(
    State(expected_token): State<Arc<str>>,
    request: Request,
    next: Next,
) -> Response {
    if request.headers().contains_key(header::ORIGIN) {
        return ApiError {
            status: StatusCode::FORBIDDEN,
            code: "browser_origin_forbidden",
            message: "browser origins must use the Desktop Core Client".to_owned(),
        }
        .into_response();
    }

    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|provided| provided == expected_token.as_ref());

    if authorized {
        next.run(request).await
    } else {
        ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "a valid Core session token is required".to_owned(),
        }
        .into_response()
    }
}

#[derive(Clone)]
struct ApiState {
    projects: Arc<ProjectService>,
    runtime: Option<Arc<dyn CreativeRuntime>>,
    handoff: Option<Arc<dyn HandoffSink>>,
    preview: Option<Arc<dyn PreviewSource>>,
    backup: Option<Arc<dyn ProjectBackupSink>>,
    llm_connections: Option<Arc<dyn LlmConnectionControl>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetBriefRequest {
    expected_revision: u64,
    brief: CreativeBriefDraft,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureLlmConnectionRequest {
    provider_kind: String,
    model: Option<String>,
    base_url: Option<String>,
    api_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectLlmModelRequest {
    model: String,
    #[serde(rename = "modelEffort", alias = "thinkingLevel")]
    thinking_level: ThinkingLevel,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedRevisionRequest {
    expected_revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApproveAgentRunRequest {
    expected_revision: u64,
    approval: CostApproval,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectCandidateRequest {
    expected_revision: u64,
    start_micros: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    core_version: &'static str,
    protocol_version: &'static str,
    schema_version: &'static str,
}
