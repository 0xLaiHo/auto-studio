//! Cross-DAW symbolic handoff precursor kept separate from frozen Q0 inputs.

mod compiler;
mod constants;
mod error;
mod evidence;
mod instrument;
mod qualification;
mod review;

pub use compiler::compile_portable_smf;
pub use constants::REVIEW_BRIEF_IDS;
pub use error::{HandoffError, InstrumentError, QualificationError};
pub use evidence::{ArtifactRecord, EvidenceManifest, write_portable_handoff};
pub use instrument::{
    AssignmentSource, InstrumentAssignment, InstrumentAssignmentManifest, InstrumentCatalog,
    InstrumentLibrary, InstrumentProfile, MidiInstrumentProfile, resolve_instrument_assignments,
    resolve_with_catalog,
};
pub use qualification::{
    CheckStatus, EvidenceArtifact, MarkerObservation, ProgramChangeObservation,
    QualificationChecks, QualificationEvidence, QualificationOutcome, QualificationPlan,
    QualificationResults, QualificationSummary, QualificationTarget, QualificationTargets,
    TargetReadiness, TargetResult, TargetSummary, prepare_qualification_matrix,
    verify_qualification_matrix,
};
pub use review::{
    ContentReviewArtifact, ContentReviewAudio, ContentReviewManifest, ContentReviewRequest,
    ContentReviewSample, ContentReviewVerification, LocalReviewAsset, prepare_content_review_pack,
    verify_content_review_pack,
};
