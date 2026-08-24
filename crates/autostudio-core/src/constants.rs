//! Domain validation limits shared by Core modules.

pub const MAX_PROJECT_NAME_CHARS: usize = 128;
pub const MAX_BRIEF_SUMMARY_CHARS: usize = 4_000;
pub const MIN_GENERATION_DURATION_SECONDS: u32 = 1;
pub const MAX_GENERATION_DURATION_SECONDS: u32 = 900;
pub const MIN_GENERATION_CANDIDATES: u8 = 1;
pub const MAX_GENERATION_CANDIDATES: u8 = 4;
