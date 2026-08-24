use std::time::Duration;

pub const ENV_CORE_BINARY: &str = "AUTOSTUDIO_CORE_BINARY";
pub const ENV_DISCOVERY_FILE: &str = "AUTOSTUDIO_DISCOVERY_FILE";
pub const ENV_PROJECT_PACKAGE: &str = "AUTOSTUDIO_PROJECT_PACKAGE";
pub const ENV_MANAGE_CORE: &str = "AUTOSTUDIO_MANAGE_CORE";
pub const ENV_BIND: &str = "AUTOSTUDIO_BIND";
pub const ENV_PARENT_HEARTBEAT: &str = "AUTOSTUDIO_PARENT_HEARTBEAT";
pub const CORE_LOOPBACK_BIND: &str = "127.0.0.1:0";
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
pub const MAX_PREVIEW_BYTES: u64 = 128 * 1024 * 1024;
