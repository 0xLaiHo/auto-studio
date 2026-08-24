use autostudio_api::discovery::DiscoveryError;
use serde::Deserialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("Auto Studio home directory could not be resolved; set AUTOSTUDIO_HOME")]
    HomeUnavailable,
    #[error("Auto Studio TUI requires an interactive terminal; use --help for usage")]
    InteractiveTerminalRequired,
    #[error("unsupported argument '{0}'; use autostudio --help")]
    UnsupportedArgument(String),
    #[error("Core binary path has no parent directory")]
    CoreBinaryWithoutParent,
    #[error("failed to start Core binary {binary}: {source}")]
    CoreSpawn {
        binary: PathBuf,
        source: std::io::Error,
    },
    #[error("Core exited before becoming ready with status {status:?}; details: {log}")]
    CoreExited { status: Option<i32>, log: PathBuf },
    #[error("Core did not become ready within the startup deadline; details: {log}")]
    CoreStartTimeout { log: PathBuf },
    #[error("Core discovery is unavailable or unsafe: {0}")]
    Discovery(#[from] DiscoveryError),
    #[error("Core endpoint must be an explicit HTTP loopback address")]
    InvalidEndpoint,
    #[error("TUI protocol {client} is incompatible with Core protocol {core}")]
    ProtocolMismatch { client: String, core: String },
    #[error("Core request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Core rejected the request ({status}, {code}): {message}")]
    CoreRejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("terminal operation failed: {0}")]
    Terminal(#[from] std::io::Error),
    #[error("this action requires an open Project")]
    ProjectRequired,
    #[error("this action requires an Agent Run")]
    RunRequired,
    #[error("this action requires a Candidate")]
    CandidateRequired,
}

impl TuiError {
    #[must_use]
    pub fn is_project_not_found(&self) -> bool {
        matches!(
            self,
            Self::CoreRejected {
                code,
                ..
            } if code == "project_not_found"
        )
    }
}

#[derive(Deserialize)]
pub(crate) struct CoreErrorBody {
    pub(crate) code: String,
    pub(crate) message: String,
}
