use autostudio_core::project::{ProjectError, ProjectStoreError};
use autostudio_core::provider::ProviderConnectionError;
use autostudio_core::runtime::CreativeRuntimeError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("discovery path must have a parent directory")]
    MissingParent,
    #[error("discovery record is invalid")]
    InvalidRecord,
    #[error("discovery file permissions expose the Core session token")]
    InsecurePermissions,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub(crate) struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl From<ProjectError> for ApiError {
    fn from(error: ProjectError) -> Self {
        match error {
            ProjectError::InvalidName(error) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_project_name",
                message: error.to_string(),
            },
            ProjectError::InvalidBrief(error) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_creative_brief",
                message: error.to_string(),
            },
            ProjectError::InvalidAgentRun(error) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_agent_run",
                message: error.to_string(),
            },
            ProjectError::InvalidProduction(error) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_production_command",
                message: error.to_string(),
            },
            ProjectError::RevisionExhausted => Self {
                status: StatusCode::INSUFFICIENT_STORAGE,
                code: "project_revision_exhausted",
                message: "the Project cannot accept another revision".to_owned(),
            },
            ProjectError::Handoff(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "handoff_unavailable",
                message: "DAW Handoff could not be materialized".to_owned(),
            },
            ProjectError::Backup(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "project_backup_unavailable",
                message: "Project backup could not be published".to_owned(),
            },
            ProjectError::Restore(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "project_snapshot_invalid",
                message: "Project data failed integrity validation".to_owned(),
            },
            ProjectError::Store(ProjectStoreError::AlreadyExists) => Self {
                status: StatusCode::CONFLICT,
                code: "project_already_exists",
                message: "a project already exists in this package".to_owned(),
            },
            ProjectError::Store(ProjectStoreError::NotFound) => Self {
                status: StatusCode::NOT_FOUND,
                code: "project_not_found",
                message: "the project package does not contain a project".to_owned(),
            },
            ProjectError::Store(ProjectStoreError::RevisionConflict { expected, actual }) => Self {
                status: StatusCode::CONFLICT,
                code: "project_revision_conflict",
                message: format!("expected Project revision {expected}, actual {actual}"),
            },
            ProjectError::Store(ProjectStoreError::Unavailable(_)) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "project_storage_unavailable",
                message: "project storage is unavailable".to_owned(),
            },
        }
    }
}

impl From<CreativeRuntimeError> for ApiError {
    fn from(error: CreativeRuntimeError) -> Self {
        match error {
            CreativeRuntimeError::Project(error) => error.into(),
            CreativeRuntimeError::Unavailable(message) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "creative_runtime_unavailable",
                message,
            },
            CreativeRuntimeError::UnknownOutcome(message) => Self {
                status: StatusCode::CONFLICT,
                code: "generation_unknown_outcome",
                message,
            },
            CreativeRuntimeError::Rejected(message) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "creative_runtime_rejected",
                message,
            },
        }
    }
}

impl From<ProviderConnectionError> for ApiError {
    fn from(error: ProviderConnectionError) -> Self {
        match error {
            ProviderConnectionError::NotConfigured
            | ProviderConnectionError::InvalidConfiguration(_) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_llm_connection",
                message: error.to_string(),
            },
            ProviderConnectionError::StorageUnavailable(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "llm_connection_storage_unavailable",
                message: error.to_string(),
            },
            ProviderConnectionError::CatalogUnavailable(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "llm_model_catalog_unavailable",
                message: error.to_string(),
            },
            ProviderConnectionError::ModelNotAvailable(_) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "llm_model_not_available",
                message: error.to_string(),
            },
            ProviderConnectionError::ThinkingLevelNotAvailable { .. } => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "llm_thinking_level_not_available",
                message: error.to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                code: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
}
