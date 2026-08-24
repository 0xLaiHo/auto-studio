use std::time::Duration;

pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: &str = "0.3.0";
pub const SCHEMA_VERSION: &str = "1";
pub(crate) const OPENAPI_V1: &str = include_str!("../../../docs/api/openapi-v1.json");
pub(crate) const MINIMUM_SESSION_TOKEN_BYTES: usize = 32;
pub(crate) const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(250);
