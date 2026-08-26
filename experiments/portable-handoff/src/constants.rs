pub const ASSIGNMENTS_FILE: &str = "instrument-assignments.json";
pub const ASSIGNMENTS_SCHEMA_VERSION: &str = "portable-instrument-assignments-v1";
pub const CATALOG_JSON: &str = include_str!("../environment/instrument-catalog-portable-v1.json");
pub const EVIDENCE_SCHEMA_VERSION: &str = "portable-handoff-evidence-v1";
pub const GM_PERCUSSION_CHANNEL: u8 = 10;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const MIDI_CHANNEL_MAX: u8 = 16;
pub const MIDI_CHANNEL_MIN: u8 = 1;
pub const MIDI_FILE: &str = "composition.mid";
pub const MIDI_MAX: u8 = 127;
pub const QUALIFICATION_PLAN_FILE: &str = "qualification-plan.json";
pub const QUALIFICATION_PLAN_SCHEMA_VERSION: &str = "daw-qualification-plan-v1";
pub const QUALIFICATION_REQUIRED_CHECKS: [&str; 8] = [
    "import_without_repair",
    "semantic_tracks_preserved",
    "tempo_meter_preserved",
    "midi_events_preserved_before_edit",
    "markers_not_lost",
    "program_change_observed",
    "save_reopen",
    "intentional_edit_export",
];
pub const QUALIFICATION_RESULTS_FILE: &str = "qualification-results.json";
pub const QUALIFICATION_RESULTS_SCHEMA_VERSION: &str = "daw-qualification-results-v1";
pub const QUALIFICATION_SUMMARY_SCHEMA_VERSION: &str = "daw-qualification-summary-v1";
pub const QUALIFICATION_TARGETS_SCHEMA_VERSION: &str = "daw-qualification-targets-v1";
pub const SPEC_FILE: &str = "spec.json";
pub const REVIEW_BRIEF_IDS: [&str; 6] = [
    "l4-song-neon",
    "l4-song-intimate",
    "l4-video-chase",
    "l4-video-emotional",
    "l4-orchestral-argument",
    "l4-electronic-microcity",
];
pub(crate) const REVIEW_SCHEMA_VERSION: &str = "q0-content-review-pack-v1";
pub(crate) const PREVIEW_SAMPLE_RATE: u32 = 48_000;
pub(crate) const PREVIEW_CHANNELS: u16 = 2;
pub(crate) const PREVIEW_FILE: &str = "preview.wav";
pub(crate) const REVIEW_MANIFEST_FILE: &str = "review-manifest.json";
pub(crate) const FEEDBACK_FILE: &str = "feedback.json";
pub(crate) const FEEDBACK_TEMPLATE_FILE: &str = "feedback-template.json";
