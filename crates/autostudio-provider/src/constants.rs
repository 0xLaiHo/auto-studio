//! Stable provider identifiers, configuration keys, and protocol names.

use std::time::Duration;

pub const PROVIDER_DEEPSEEK: &str = "deepseek";
pub const PROVIDER_OPENAI: &str = "openai";
pub const PROVIDER_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_KIMI_OPEN: &str = "kimi-open";
pub const PROVIDER_KIMI_CODE: &str = "kimi-code";

pub const PROTOCOL_OPENAI_CHAT_COMPLETIONS: &str = "openai_chat_completions";
pub const PROTOCOL_OPENAI_RESPONSES: &str = "openai_responses";
pub const PROTOCOL_ANTHROPIC_MESSAGES: &str = "anthropic_messages";

pub const ENV_LLM_PROVIDER: &str = "AUTOSTUDIO_LLM_PROVIDER";
pub const ENV_LLM_CONNECTION_FILE: &str = "AUTOSTUDIO_LLM_CONNECTION_FILE";
pub const ENV_DEEPSEEK_API_KEY: &str = "DEEPSEEK_API_KEY";
pub const ENV_DEEPSEEK_BASE_URL: &str = "DEEPSEEK_BASE_URL";
pub const ENV_DEEPSEEK_MODEL: &str = "DEEPSEEK_MODEL";
pub const ENV_OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const ENV_OPENAI_BASE_URL: &str = "OPENAI_BASE_URL";
pub const ENV_OPENAI_MODEL: &str = "OPENAI_MODEL";
pub const ENV_ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
pub const ENV_ANTHROPIC_BASE_URL: &str = "ANTHROPIC_BASE_URL";
pub const ENV_ANTHROPIC_MODEL: &str = "ANTHROPIC_MODEL";
pub const ENV_MOONSHOT_API_KEY: &str = "MOONSHOT_API_KEY";
pub const ENV_MOONSHOT_BASE_URL: &str = "MOONSHOT_BASE_URL";
pub const ENV_MOONSHOT_MODEL: &str = "MOONSHOT_MODEL";
pub const ENV_KIMI_CODE_API_KEY: &str = "KIMI_CODE_API_KEY";
pub const ENV_KIMI_CODE_BASE_URL: &str = "KIMI_CODE_BASE_URL";
pub const ENV_KIMI_CODE_MODEL: &str = "KIMI_CODE_MODEL";

pub const DEFAULT_DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_MOONSHOT_BASE_URL: &str = "https://api.moonshot.cn/v1";
pub const DEFAULT_KIMI_CODE_BASE_URL: &str = "https://api.kimi.com/coding/";

pub const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-v4-flash";
pub const DEFAULT_MOONSHOT_MODEL: &str = "kimi-k3";
pub const DEFAULT_KIMI_CODE_MODEL: &str = "k3-256k";
pub const KIMI_CODE_CATALOG_SNAPSHOT_DATE: &str = "2026-08-22";
pub const THINKING_CAPABILITY_REVISION: &str = "autostudio.thinking-capability/1";
pub const THINKING_MAPPING_REVISION: &str = "autostudio.thinking-mapping/1";

pub const OPENAI_RESPONSES_PATH: &str = "responses";
pub const CHAT_COMPLETIONS_PATH: &str = "chat/completions";
pub const ANTHROPIC_MESSAGES_PATH: &str = "v1/messages";
pub const OPENAI_MODELS_PATH: &str = "models";
pub const ANTHROPIC_MODELS_PATH: &str = "v1/models?limit=1000";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const PLAN_TOOL_NAME: &str = "submit_creative_plan";
pub const PLAN_SYSTEM_PROMPT: &str = "You are the planning model inside Auto Studio. Return one concise, creator-visible music generation plan. Preserve the brief's intent, keep duration between 1 and 900 seconds, and request between 1 and 4 candidates. When a submit_creative_plan tool is available, call it exactly once. Do not reveal private chain-of-thought. Return only the requested structured fields.";

pub const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
pub const MAX_PROVIDER_ERROR_CHARS: usize = 1_024;
pub const PLAN_MAX_OUTPUT_TOKENS: u32 = 4_096;
pub const LEGACY_THINKING_HIGH_TOKENS: u32 = 8_192;
pub const LEGACY_THINKING_MAX_TOKENS: u32 = 16_384;
pub const LLM_CONNECTION_SCHEMA_V1: &str = "autostudio.llm-connection/1";
pub const LLM_CONNECTION_SCHEMA_V2: &str = "autostudio.llm-connection/2";
pub const LLM_CONNECTION_SCHEMA_V3: &str = "autostudio.llm-connection/3";
pub const LLM_CONNECTION_SCHEMA: &str = "autostudio.llm-connection/4";
pub const MAX_LLM_CONNECTION_FILE_BYTES: u64 = 1_024 * 1_024;
