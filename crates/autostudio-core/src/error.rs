use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ContextStoreError {
    #[error("context journal revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("context journal is corrupt: {0}")]
    Corrupt(String),
    #[error("context storage is unavailable: {0}")]
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ContextError {
    #[error("{0} identity is invalid")]
    InvalidId(&'static str),
    #[error("context field '{0}' must not be empty")]
    EmptyField(&'static str),
    #[error("context JSON field '{0}' is invalid")]
    InvalidJson(&'static str),
    #[error("model-visible Tool name must match ^[a-zA-Z0-9_-]{{1,64}}$")]
    InvalidToolName,
    #[error("context digest is invalid")]
    InvalidDigest,
    #[error("context token budget is invalid")]
    InvalidTokenBudget,
    #[error("context item sequence is exhausted")]
    SequenceExhausted,
    #[error("context journal violates an ordering or identity invariant: {0}")]
    InconsistentJournal(String),
    #[error("system clock is before the Unix epoch")]
    InvalidClock,
    #[error("context serialization failed: {0}")]
    Serialization(String),
    #[error(
        "context compaction is required before inference: estimated {estimated_tokens} input tokens, budget {input_budget_tokens}"
    )]
    CompactionRequired {
        estimated_tokens: u64,
        input_budget_tokens: u64,
    },
    #[error("automatic context compaction could not produce a bounded model surface")]
    AutomaticCompactionUnavailable,
    #[error("Provider context overflow recovery was already attempted for this Run")]
    OverflowRecoveryExhausted,
    #[error("context retrieval query is invalid: {0}")]
    InvalidRetrievalQuery(&'static str),
    #[error("context retrieval result is invalid: {0}")]
    InvalidRetrievalResult(&'static str),
    #[error(transparent)]
    Surface(#[from] ContextSurfaceError),
    #[error(transparent)]
    Store(#[from] ContextStoreError),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CompactionError {
    #[error("Compaction identity is invalid")]
    InvalidId,
    #[error("Compaction source journal revision must be greater than zero")]
    InvalidSourceRevision,
    #[error("Compaction must replace at least one transcript item")]
    EmptyReplacementSet,
    #[error("Compaction replacement items must be unique and exclude the first kept item")]
    InvalidReplacementSet,
    #[error("Compaction summary field '{0}' must not be empty")]
    EmptySummaryField(&'static str),
    #[error("Compaction summary field '{0}' exceeds the bounded size")]
    SummaryFieldTooLong(&'static str),
    #[error("Compaction summary contains too many list items")]
    TooManySummaryItems,
    #[error("Compaction format revision is unsupported")]
    UnsupportedFormat,
    #[error("Compaction content hash does not match its source facts")]
    ContentHashMismatch,
    #[error("Compaction serialization failed: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ContextSurfaceError {
    #[error("Context Surface footprint is internally inconsistent")]
    InvalidFootprint,
    #[error("Context Surface spill content must not be empty")]
    EmptySpillContent,
    #[error("Context Surface spill reference is invalid")]
    InvalidSpillReference,
    #[error("Context Surface spill content hash does not match its bytes")]
    SpillHashMismatch,
    #[error("Context Surface footprint exceeds the supported numeric range")]
    FootprintOverflow,
    #[error("Context Surface serialization failed: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ContinuityError {
    #[error("Provider Continuity identity is invalid")]
    InvalidId,
    #[error("Provider Continuity field '{0}' must not be empty")]
    EmptyField(&'static str),
    #[error("Provider Continuity digest is invalid")]
    InvalidDigest,
    #[error("Provider Continuity expiry must be later than creation")]
    InvalidExpiry,
    #[error("Provider Continuity serialization failed: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AgentRunError {
    #[error("Agent Run id is invalid")]
    InvalidId,
    #[error("Agent Plan summary must not be empty")]
    EmptyPlanSummary,
    #[error("Agent Plan input hash must not be empty")]
    EmptyInputHash,
    #[error("music generation prompt must not be empty")]
    EmptyGenerationPrompt,
    #[error("music generation duration must be between 1 and 900 seconds")]
    InvalidDuration,
    #[error("music generation must request between 1 and 4 Candidates")]
    InvalidCandidateCount,
    #[error("cost estimate is invalid")]
    InvalidCostEstimate,
    #[error("inference usage and actual cost are inconsistent")]
    InvalidUsage,
    #[error("inference Provider provenance is invalid")]
    InvalidInferenceProvenance,
    #[error("Approval no longer matches the planned input")]
    ApprovalInputChanged,
    #[error("Approval does not cover the estimated maximum cost")]
    ApprovalBudgetTooLow,
    #[error("Agent Run state does not allow this transition")]
    InvalidTransition,
    #[error("Agent Run was not found")]
    NotFound,
    #[error("Generation Job does not match the approved Agent Plan")]
    InvalidGenerationJob,
    #[error("Agent Run failure record is invalid")]
    InvalidFailure,
    #[error("Agent Plan expected {expected} Candidates, received {actual}")]
    CandidateCountMismatch { expected: usize, actual: usize },
    #[error("Project already contains an active Agent Run")]
    ActiveRunExists,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProductionError {
    #[error("production identity is invalid")]
    InvalidId,
    #[error("Candidate label must not be empty")]
    EmptyCandidateLabel,
    #[error("Asset path must be a relative path inside assets")]
    UnsafeAssetPath,
    #[error("Ship 0 supports committed WAV audio only")]
    UnsupportedMediaType,
    #[error("Asset SHA-256 is invalid")]
    InvalidAssetHash,
    #[error("audio metadata is invalid or unsupported")]
    InvalidAudioMetadata,
    #[error("Provenance Record is incomplete")]
    IncompleteProvenance,
    #[error("at least one Candidate is required")]
    EmptyCandidates,
    #[error("Candidate was not found")]
    CandidateNotFound,
    #[error("Asset Version was not found")]
    AssetNotFound,
    #[error("a Creator Selection is required before DAW Handoff")]
    MissingSelection,
    #[error("DAW Handoff path must stay inside the package exports directory")]
    UnsafeHandoffPath,
    #[error("DAW Handoff must contain at least one materialized file")]
    EmptyHandoff,
    #[error("DAW Handoff file metadata is invalid")]
    InvalidHandoffFile,
    #[error("Audio Clip Timeline is inconsistent with the current Selection")]
    InvalidTimeline,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProjectNameError {
    #[error("project name must not be empty")]
    Empty,
    #[error("project name must contain at most {max_chars} characters")]
    TooLong { max_chars: usize },
    #[error("project name must be stored in normalized form")]
    NotNormalized,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CreativeBriefError {
    #[error("creative brief summary must not be empty")]
    EmptySummary,
    #[error("creative brief summary must contain at most {max_chars} characters")]
    SummaryTooLong { max_chars: usize },
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProjectRestoreError {
    #[error("stored project id is invalid")]
    InvalidId,
    #[error(transparent)]
    InvalidName(#[from] ProjectNameError),
    #[error(transparent)]
    InvalidBrief(#[from] CreativeBriefError),
    #[error(transparent)]
    InvalidAgentRun(#[from] AgentRunError),
    #[error(transparent)]
    InvalidProduction(#[from] ProductionError),
    #[error("stored Project snapshot contains inconsistent references or revisions")]
    InconsistentSnapshot,
    #[error("Project backup name is unsafe")]
    UnsafeBackupName,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProjectStoreError {
    #[error("a project already exists in this package")]
    AlreadyExists,
    #[error("the project package does not contain a project")]
    NotFound,
    #[error("project revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("project storage is unavailable: {0}")]
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProjectError {
    #[error(transparent)]
    InvalidName(#[from] ProjectNameError),
    #[error(transparent)]
    InvalidBrief(#[from] CreativeBriefError),
    #[error(transparent)]
    InvalidAgentRun(#[from] AgentRunError),
    #[error(transparent)]
    InvalidProduction(#[from] ProductionError),
    #[error("project revision space is exhausted")]
    RevisionExhausted,
    #[error("DAW Handoff is unavailable: {0}")]
    Handoff(String),
    #[error("Project backup is unavailable: {0}")]
    Backup(String),
    #[error(transparent)]
    Restore(#[from] ProjectRestoreError),
    #[error(transparent)]
    Store(#[from] ProjectStoreError),
}

#[derive(Debug, Error)]
pub enum CreativeRuntimeError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("Agent runtime is unavailable: {0}")]
    Unavailable(String),
    #[error("Provider outcome is unknown and requires reconciliation: {0}")]
    UnknownOutcome(String),
    #[error("Agent runtime rejected the command: {0}")]
    Rejected(String),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProviderConnectionError {
    #[error("LLM Provider is not configured")]
    NotConfigured,
    #[error("Provider Connection configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("Provider Connection storage is unavailable: {0}")]
    StorageUnavailable(String),
    #[error("Provider model catalog is unavailable: {0}")]
    CatalogUnavailable(String),
    #[error("Provider model '{0}' is not available in the current catalog")]
    ModelNotAvailable(String),
    #[error("thinking level '{level}' is not available for Provider model '{model}'")]
    ThinkingLevelNotAvailable { model: String, level: String },
}
