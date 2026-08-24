use std::time::Duration;

use ratatui::style::Color;

pub const ENV_DISCOVERY_FILE: &str = "AUTOSTUDIO_DISCOVERY_FILE";
pub const ENV_AUTOSTUDIO_HOME: &str = "AUTOSTUDIO_HOME";
pub const ENV_PROJECT_PACKAGE: &str = "AUTOSTUDIO_PROJECT_PACKAGE";
pub const ENV_CORE_BINARY: &str = "AUTOSTUDIO_CORE_BINARY";
pub const ENV_BIND: &str = "AUTOSTUDIO_BIND";
pub const ENV_PARENT_HEARTBEAT: &str = "AUTOSTUDIO_PARENT_HEARTBEAT";
pub const ENV_LLM_CONNECTION_FILE: &str = "AUTOSTUDIO_LLM_CONNECTION_FILE";
pub const CORE_LOOPBACK_BIND: &str = "127.0.0.1:0";
pub const CORE_BINARY_NAME: &str = if cfg!(windows) {
    "core-daemon.exe"
} else {
    "core-daemon"
};
pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub const CORE_START_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub const CORE_START_TIMEOUT: Duration = Duration::from_secs(10);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
pub const DEFAULT_APPROVAL_CURRENCY: &str = "USD";
pub const DEFAULT_PROJECT_NAME: &str = "Untitled Project";
pub const MAX_LOG_ENTRIES: usize = 5;
// Cyberpunk theme tokens. Keep all terminal color decisions here so every
// surface preserves the same semantic contrast across true-color terminals.
pub const THEME_CANVAS: Color = Color::Rgb(6, 8, 22);
pub const THEME_SURFACE: Color = Color::Rgb(16, 22, 42);
pub const THEME_OVERLAY: Color = Color::Rgb(23, 26, 51);
pub const THEME_TEXT: Color = Color::Rgb(234, 247, 255);
pub const THEME_MUTED: Color = Color::Rgb(126, 142, 173);
pub const THEME_PRIMARY: Color = Color::Rgb(22, 242, 208);
pub const THEME_SECONDARY: Color = Color::Rgb(139, 92, 246);
pub const THEME_HIGHLIGHT: Color = Color::Rgb(255, 79, 216);
pub const THEME_WARNING: Color = Color::Rgb(255, 209, 102);
pub const THEME_DANGER: Color = Color::Rgb(255, 59, 129);
pub const THEME_SUCCESS: Color = Color::Rgb(57, 255, 136);
pub const THEME_INK: Color = Color::Rgb(5, 6, 17);
pub const DEFAULT_HOME_DIRECTORY: &str = "auto-studio";
pub const DEFAULT_PROJECT_DIRECTORY: &str = "projects/default.autostudio";
pub const DEFAULT_RUNTIME_DIRECTORY: &str = "runtime";
pub const DEFAULT_DISCOVERY_FILE: &str = "runtime/core.json";
pub const DEFAULT_CORE_LOG_FILE: &str = "runtime/core-startup.log";
pub const DEFAULT_LLM_CONNECTION_FILE: &str = "config/llm-connection.json";
pub const HELP_TEXT: &str = "Auto Studio local Creative Agent\n\nUsage:\n  autostudio          Open the interactive TUI\n  autostudio --help   Show this help\n  autostudio --version\n\nInside the TUI, type / to open commands, /connect to save a Provider key, and /model to select a fetched model.";
