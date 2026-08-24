use autostudio_core::project::ProjectError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentPlannerError {
    #[error("Project must contain a Creative Brief before starting an Agent Run")]
    MissingBrief,
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Provider rejected the request: {0}")]
    Rejected(String),
    #[error("Provider outcome is unknown: {0}")]
    UnknownOutcome(String),
    #[error("Provider response is invalid: {0}")]
    InvalidResponse(String),
    #[error("Provider is unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Error)]
pub enum GenerationCoordinatorError {
    #[error("Agent Run was not found")]
    RunNotFound,
    #[error("Agent Run requires Creator Approval before generation")]
    RunNotApproved,
    #[error("Agent Run is not waiting for Unknown Outcome reconciliation")]
    RunNotUnknown,
    #[error("Agent Run does not have a submitted Generation Job")]
    RunNotSubmitted,
    #[error("Agent Run does not contain a durable Generation Attempt")]
    MissingAttempt,
    #[error("Agent Run does not contain a durable Generation Job")]
    MissingJob,
    #[error("the configured Music Provider does not own the Generation Attempt")]
    WrongAdapter,
    #[error("Provider reconciliation returned a mismatched Attempt")]
    MismatchedReconciliation,
    #[error("generated Asset commit failed: {0}")]
    AssetCommit(String),
    #[error("Agent Plan expected {expected} Candidates, received {actual}")]
    CandidateCountMismatch { expected: usize, actual: usize },
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, Error)]
pub enum ProviderConfigError {
    #[error("unsupported LLM Provider '{0}'")]
    UnsupportedProvider(String),
    #[error("{provider} requires environment variable {variable}")]
    MissingSetting {
        provider: String,
        variable: &'static str,
    },
    #[error("{provider} has an invalid base URL in {variable}")]
    InvalidBaseUrl {
        provider: String,
        variable: &'static str,
    },
    #[error("failed to build the Provider HTTP client: {0}")]
    HttpClient(String),
}

#[derive(Debug, Error)]
pub enum ConnectionStoreError {
    #[error("Provider Connection path must have a parent directory")]
    MissingParent,
    #[error("Provider Connection file has insecure permissions")]
    InsecurePermissions,
    #[error("Provider Connection file is too large")]
    FileTooLarge,
    #[error("Provider Connection file uses an unsupported schema")]
    UnsupportedSchema,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
