use autostudio_core::context::ContextError;
use autostudio_core::continuity::ContinuityError;
use autostudio_core::project::ProjectError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentPlannerError {
    #[error("Project must contain a Creative Brief before starting an Agent Run")]
    MissingBrief,
    #[error("Agent Run was not found")]
    RunNotFound,
    #[error("Agent Run is not in Planning state")]
    RunNotPlanning,
    #[error("planning exceeded the bounded Inference Turn limit")]
    TurnLimitExceeded,
    #[error(
        "a prepared Inference Turn has no durable Provider output; automatic resubmission is unsafe"
    )]
    InterruptedTurn,
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Continuity(#[from] ContinuityVaultError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    PlanningTool(#[from] PlanningToolError),
}

#[derive(Debug, Error)]
pub enum PlanningToolError {
    #[error("planning Tool '{0}' is not available")]
    UnknownTool(String),
    #[error("planning Tool arguments are invalid: {0}")]
    InvalidArguments(String),
    #[error("planning Tool state is inconsistent: {0}")]
    InconsistentState(String),
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
    #[error("Provider Continuity is unavailable: {0}")]
    ContinuityUnavailable(String),
    #[error("Provider context window is exhausted: {0}")]
    ContextOverflow(String),
}

pub(crate) fn is_context_overflow_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    crate::constants::CONTEXT_OVERFLOW_ERROR_SIGNALS
        .iter()
        .any(|signal| normalized.contains(signal))
}

pub(crate) fn invalid_or_context_overflow(message: String) -> AdapterError {
    if is_context_overflow_error(&message) {
        AdapterError::ContextOverflow(message)
    } else {
        AdapterError::InvalidResponse(message)
    }
}

#[derive(Debug, Error)]
pub enum ContinuityVaultError {
    #[error("Provider Continuity path must have a parent directory")]
    MissingParent,
    #[error("Provider Continuity storage must remain outside the Project Package")]
    InsideProject,
    #[error("Provider Continuity storage has insecure permissions")]
    InsecurePermissions,
    #[error("Provider Continuity entry is too large")]
    FileTooLarge,
    #[error("Provider Continuity entry uses an unsupported schema")]
    UnsupportedSchema,
    #[error("Provider Continuity entry is corrupt")]
    Corrupt,
    #[error("Provider Continuity cryptography failed")]
    Crypto,
    #[error("Provider Continuity clock is invalid")]
    InvalidClock,
    #[error(transparent)]
    Domain(#[from] ContinuityError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
#[cfg(feature = "legacy-generation")]
pub enum GenerationCoordinatorError {
    #[error("Agent Run was not found")]
    RunNotFound,
    #[error("Agent Run does not contain a completed Agent Plan")]
    MissingPlan,
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
