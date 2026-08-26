//! Domain validation limits shared by Core modules.

pub const MAX_PROJECT_NAME_CHARS: usize = 128;
pub const MAX_BRIEF_SUMMARY_CHARS: usize = 4_000;
pub const MIN_GENERATION_DURATION_SECONDS: u32 = 1;
pub const MAX_GENERATION_DURATION_SECONDS: u32 = 900;
pub const MAX_PROVIDER_TOOL_NAME_CHARS: usize = 64;
pub const MAX_COMPACTION_SUMMARY_FIELD_CHARS: usize = 16_000;
pub const MAX_COMPACTION_SUMMARY_ITEMS: usize = 256;
pub const MIN_GENERATION_CANDIDATES: u8 = 1;
pub const MAX_GENERATION_CANDIDATES: u8 = 4;
pub const CONTINUITY_BINDING_FORMAT_REVISION: &str = "autostudio.continuity-binding/1";
pub const COMPACTION_FORMAT_REVISION: &str = "autostudio.compaction/1";
