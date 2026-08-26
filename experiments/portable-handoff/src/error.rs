#[derive(Debug, thiserror::Error)]
pub enum InstrumentError {
    #[error("invalid portable instrument catalog JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid portable instrument catalog: {0}")]
    Validation(String),
    #[error("not enough General MIDI channels for track `{track_id}`")]
    ChannelExhausted { track_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum QualificationError {
    #[error("invalid DAW qualification input: {0}")]
    InvalidInput(String),
    #[error("qualification artifact `{path}` has SHA-256 {actual}, expected {expected}")]
    HashMismatch {
        path: String,
        actual: String,
        expected: String,
    },
    #[error("qualification evidence path `{0}` is not a safe relative path")]
    UnsafeEvidencePath(String),
    #[error("qualification evidence `{0}` is missing")]
    MissingEvidence(String),
}

#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error(transparent)]
    Spec(#[from] autostudio_music_quality::SpecError),
    #[error(transparent)]
    BaseCompile(#[from] autostudio_music_quality::CompileError),
    #[error(transparent)]
    Instrument(#[from] InstrumentError),
    #[error(transparent)]
    Qualification(#[from] QualificationError),
    #[error("failed to parse the base Type-1 MIDI: {0}")]
    MidiParse(String),
    #[error("failed to encode portable Type-1 MIDI: {0}")]
    MidiEncode(String),
    #[error("base MIDI track count {actual} does not match expected {expected}")]
    TrackCount { actual: usize, expected: usize },
    #[error("assignment track `{assignment_track_id}` does not match `{spec_track_id}`")]
    AssignmentTrackMismatch {
        assignment_track_id: String,
        spec_track_id: String,
    },
    #[error("invalid instrument assignment for track `{track_id}`: {reason}")]
    InvalidAssignment { track_id: String, reason: String },
    #[error("base MIDI track {track_index} does not start with a track name")]
    MissingTrackName { track_index: usize },
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("artifact serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}
