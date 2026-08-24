#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("invalid ExperimentalMusicSpec JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ExperimentalMusicSpec validation failed: {0}")]
    Validation(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("cannot compile invalid ExperimentalMusicSpec: {0}")]
    InvalidSpec(String),
    #[error("MIDI timeline exceeds the SMF delta range")]
    TimelineOverflow,
    #[error("failed to encode SMF MIDI: {0}")]
    Encode(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error(transparent)]
    Spec(#[from] SpecError),
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("artifact serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("invalid Q0 experiment input: {0}")]
    InvalidInput(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("invalid DeepSeek configuration: {0}")]
    InvalidConfig(String),
    #[error("failed to create DeepSeek HTTP client: {0}")]
    HttpClient(String),
    #[error("DeepSeek transport failed: {0}")]
    Transport(String),
    #[error("DeepSeek returned HTTP {status}: {message}")]
    HttpStatus { status: u16, message: String },
    #[error("invalid DeepSeek response: {0}")]
    InvalidResponse(String),
}
