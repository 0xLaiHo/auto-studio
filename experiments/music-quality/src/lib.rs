//! Reproducible Q0 music-quality experiment contracts.

mod blind;
mod compiler;
mod constants;
mod error;
mod evidence;
mod provider;
mod runner;
mod spec;
mod verification;

pub use blind::{BlindCandidate, BlindPackageManifest, prepare_blind_package};
pub use compiler::compile_to_smf;
pub use error::{CompileError, ExperimentError, ProviderError, SpecError};
pub use evidence::{ArtifactRecord, EvidenceManifest, write_compilation_evidence};
pub use provider::{DeepSeekClient, ProviderTurn, ProviderUsage};
pub use runner::{
    Corpus, ExperimentRun, FrozenBrief, ProtocolBindingEvidence, RunMode, RunPolicy,
    default_assets_root, load_corpus, resume_mode_b_revision, resume_mode_b_with_policy, run_brief,
    run_brief_with_policy,
};
pub use spec::{
    ControlChange, ExperimentalMusicSpec, KeyChange, MidiNote, MusicRegion, MusicSection,
    MusicTrack, PitchRegister, TempoChange, TimeSignature,
};
pub use verification::{FormalSummary, verify_formal, verify_formal_with_protocol};
