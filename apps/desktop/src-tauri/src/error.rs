use std::path::PathBuf;

use autostudio_api::discovery::DiscoveryError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManagedCoreError {
    #[error("Core discovery path must have a parent directory")]
    DiscoveryWithoutParent,
    #[error("Desktop executable path must have a parent directory")]
    BinaryWithoutParent,
    #[error("Core binary was not found at {0}")]
    BinaryNotFound(PathBuf),
    #[error("failed to spawn Core binary {binary}: {source}")]
    Spawn {
        binary: PathBuf,
        source: std::io::Error,
    },
    #[error("managed Core filesystem operation failed: {0}")]
    Io(std::io::Error),
}

#[derive(Debug, Error)]
pub enum CoreClientError {
    #[error("Core discovery is unavailable or unsafe: {0}")]
    Discovery(#[from] DiscoveryError),
    #[error("Core endpoint must be an explicit HTTP loopback address")]
    InvalidEndpoint,
    #[error("Desktop protocol {desktop} is incompatible with Core protocol {core}")]
    ProtocolMismatch { desktop: String, core: String },
    #[error("Desktop schema {desktop} is incompatible with Core schema {core}")]
    SchemaMismatch { desktop: String, core: String },
    #[error("Core health check returned HTTP {0}")]
    UnexpectedHealthStatus(u16),
    #[error("Core health check did not report a healthy state")]
    UnhealthyCore,
    #[error("Preview response is not WAV audio")]
    InvalidPreviewMediaType,
    #[error("Preview response exceeds the Desktop memory limit")]
    PreviewTooLarge,
    #[error("Core request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Core rejected the request ({status}, {code}): {message}")]
    CoreRejected {
        status: u16,
        code: String,
        message: String,
    },
}

#[derive(Deserialize)]
pub(crate) struct ApiErrorResponse {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct CommandError {
    code: String,
    message: String,
}

impl From<CoreClientError> for CommandError {
    fn from(error: CoreClientError) -> Self {
        let (code, message) = match error {
            CoreClientError::Discovery(error) => (
                "core_not_discovered".to_owned(),
                format!("无法发现本地 Core：{error}"),
            ),
            CoreClientError::InvalidEndpoint => (
                "unsafe_core_endpoint".to_owned(),
                "Core 地址不是安全的本机回环地址".to_owned(),
            ),
            CoreClientError::ProtocolMismatch { desktop, core } => (
                "protocol_mismatch".to_owned(),
                format!("Desktop 协议 {desktop} 与 Core 协议 {core} 不兼容"),
            ),
            CoreClientError::SchemaMismatch { desktop, core } => (
                "schema_mismatch".to_owned(),
                format!("Desktop 数据结构 {desktop} 与 Core 数据结构 {core} 不兼容"),
            ),
            CoreClientError::UnexpectedHealthStatus(status) => (
                "core_unhealthy".to_owned(),
                format!("Core 健康检查返回 HTTP {status}"),
            ),
            CoreClientError::UnhealthyCore => (
                "core_unhealthy".to_owned(),
                "Core 未报告健康状态".to_owned(),
            ),
            CoreClientError::InvalidPreviewMediaType => (
                "invalid_preview_media".to_owned(),
                "Core 返回了不受支持的试听格式".to_owned(),
            ),
            CoreClientError::PreviewTooLarge => (
                "preview_too_large".to_owned(),
                "试听文件超过 Desktop 内存上限".to_owned(),
            ),
            CoreClientError::Http(error) => (
                "core_unreachable".to_owned(),
                format!("无法连接本地 Core：{error}"),
            ),
            CoreClientError::CoreRejected { code, message, .. } => (code, message),
        };
        Self { code, message }
    }
}
