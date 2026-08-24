use std::time::Duration;

pub const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:47321";
pub const DEFAULT_LLM_PROVIDER: &str = "deepseek";
pub const MAX_PARENT_HEARTBEAT_AGE: Duration = Duration::from_secs(10);
pub const PARENT_HEARTBEAT_POLL_INTERVAL: Duration = Duration::from_secs(2);
