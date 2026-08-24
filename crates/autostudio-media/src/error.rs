use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("Provider staging path escapes the approved staging directory")]
    StagingPathEscape,
    #[error("Project Asset path escapes the Project Package")]
    ProjectPathEscape,
    #[error("Project Asset content does not match its recorded SHA-256")]
    AssetHashMismatch,
    #[error("an existing DAW Handoff does not match the requested Selection")]
    ExistingHandoffConflict,
    #[error("Preview byte range is invalid or exceeds the response limit")]
    PreviewRange,
    #[error("generated WAV uses an unsupported sample format")]
    UnsupportedWav,
    #[error("generated WAV is malformed: {0}")]
    InvalidWav(hound::Error),
    #[error("DAW Handoff manifest serialization failed: {0}")]
    Manifest(serde_json::Error),
    #[error("media filesystem operation failed: {0}")]
    Io(std::io::Error),
}
