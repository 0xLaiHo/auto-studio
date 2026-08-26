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
