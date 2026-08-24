use std::env;
use std::fmt;
use std::time::{Duration, Instant};

use reqwest::header::ACCEPT_ENCODING;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::constants::{
    DEEPSEEK_CHAT_PATH, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEK_MODEL, DEFAULT_THINKING_LEVEL,
    ENV_DEEPSEEK_API_KEY, ENV_DEEPSEEK_BASE_URL, ENV_DEEPSEEK_MODEL, ENV_DEEPSEEK_THINKING_LEVEL,
    MAX_PROVIDER_ERROR_CHARS, PROVIDER_NAME, PROVIDER_PROTOCOL, PROVIDER_TIMEOUT_SECONDS,
};
use crate::error::ProviderError;

pub struct DeepSeekClient {
    client: Client,
    base_url: Url,
    api_key: String,
    model: String,
    thinking_level: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProviderTurn {
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub thinking_level: String,
    pub request: Value,
    pub response_id: Option<String>,
    pub content: String,
    pub finish_reason: Option<String>,
    pub usage: ProviderUsage,
    pub latency_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderUsage {
    pub prompt_tokens: Option<u64>,
    pub prompt_cache_hit_tokens: Option<u64>,
    pub prompt_cache_miss_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

impl ProviderUsage {
    pub(crate) fn add(&mut self, other: &Self) {
        self.prompt_tokens = sum_optional(self.prompt_tokens, other.prompt_tokens);
        self.prompt_cache_hit_tokens =
            sum_optional(self.prompt_cache_hit_tokens, other.prompt_cache_hit_tokens);
        self.prompt_cache_miss_tokens = sum_optional(
            self.prompt_cache_miss_tokens,
            other.prompt_cache_miss_tokens,
        );
        self.completion_tokens = sum_optional(self.completion_tokens, other.completion_tokens);
        self.total_tokens = sum_optional(self.total_tokens, other.total_tokens);
    }
}

impl fmt::Debug for DeepSeekClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeepSeekClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("thinking_level", &self.thinking_level)
            .finish_non_exhaustive()
    }
}

impl Drop for DeepSeekClient {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

impl DeepSeekClient {
    /// Creates a Q0 client from the frozen `DEEPSEEK_*` environment contract.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when `DEEPSEEK_API_KEY` is missing or another
    /// setting is invalid.
    pub fn from_environment() -> Result<Self, ProviderError> {
        let api_key = env::var(ENV_DEEPSEEK_API_KEY).map_err(|_| {
            ProviderError::InvalidConfig(format!("{ENV_DEEPSEEK_API_KEY} is required"))
        })?;
        let base_url = env::var(ENV_DEEPSEEK_BASE_URL)
            .unwrap_or_else(|_| DEFAULT_DEEPSEEK_BASE_URL.to_owned());
        let model =
            env::var(ENV_DEEPSEEK_MODEL).unwrap_or_else(|_| DEFAULT_DEEPSEEK_MODEL.to_owned());
        let thinking_level = env::var(ENV_DEEPSEEK_THINKING_LEVEL)
            .unwrap_or_else(|_| DEFAULT_THINKING_LEVEL.to_owned());
        Self::new(&base_url, api_key, model, thinking_level)
    }

    /// Creates the sole real inference adapter used by Q0.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for an invalid URL, empty credential/model or
    /// unsupported frozen thinking level.
    pub fn new(
        base_url: &str,
        api_key: impl Into<String>,
        model: impl Into<String>,
        thinking_level: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let mut base_url = Url::parse(base_url)
            .map_err(|error| ProviderError::InvalidConfig(error.to_string()))?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ProviderError::InvalidConfig("empty API key".to_owned()));
        }
        let model = model.into();
        if model.trim().is_empty() {
            return Err(ProviderError::InvalidConfig("empty model".to_owned()));
        }
        let thinking_level = thinking_level.into().to_ascii_lowercase();
        if !matches!(thinking_level.as_str(), "off" | "high" | "max") {
            return Err(ProviderError::InvalidConfig(format!(
                "unsupported thinking level `{thinking_level}`"
            )));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(PROVIDER_TIMEOUT_SECONDS))
            .build()
            .map_err(|error| ProviderError::HttpClient(error.to_string()))?;
        Ok(Self {
            client,
            base_url,
            api_key,
            model,
            thinking_level,
        })
    }

    /// Makes one non-streaming JSON-only `DeepSeek` request and returns a
    /// normalized, reasoning-free evidence record.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for invalid limits, transport/status failures
    /// or malformed Provider responses.
    pub async fn generate_json(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<ProviderTurn, ProviderError> {
        if max_tokens == 0 || max_tokens > 65_536 {
            return Err(ProviderError::InvalidConfig(format!(
                "max_tokens {max_tokens} is outside 1..=65536"
            )));
        }
        let mut request = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "response_format": {"type": "json_object"},
            "stream": false,
            "max_tokens": max_tokens,
            "temperature": 0.7
        });
        if self.thinking_level == "off" {
            request["thinking"] = json!({"type": "disabled"});
            request["reasoning_effort"] = json!("none");
        } else {
            request["thinking"] = json!({"type": "enabled"});
            request["reasoning_effort"] = json!(self.thinking_level);
        }
        let endpoint = self
            .base_url
            .join(DEEPSEEK_CHAT_PATH)
            .map_err(|error| ProviderError::InvalidConfig(error.to_string()))?;
        let started = Instant::now();
        let response = self
            .client
            .post(endpoint)
            .header(ACCEPT_ENCODING, "identity")
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|error| ProviderError::Transport(redact(&error.to_string(), &self.api_key)))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| ProviderError::Transport(redact(&error.to_string(), &self.api_key)))?;
        if status != StatusCode::OK {
            return Err(ProviderError::HttpStatus {
                status: status.as_u16(),
                message: truncate(&redact(&text, &self.api_key)),
            });
        }
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing choices[0].message.content".to_owned())
            })?
            .to_owned();
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(ProviderTurn {
            provider: PROVIDER_NAME.to_owned(),
            protocol: PROVIDER_PROTOCOL.to_owned(),
            model: value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(&self.model)
                .to_owned(),
            thinking_level: self.thinking_level.clone(),
            request,
            response_id: value.get("id").and_then(Value::as_str).map(str::to_owned),
            content,
            finish_reason: value
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
            usage: ProviderUsage {
                prompt_tokens: value
                    .pointer("/usage/prompt_tokens")
                    .and_then(Value::as_u64),
                prompt_cache_hit_tokens: value
                    .pointer("/usage/prompt_cache_hit_tokens")
                    .and_then(Value::as_u64),
                prompt_cache_miss_tokens: value
                    .pointer("/usage/prompt_cache_miss_tokens")
                    .and_then(Value::as_u64),
                completion_tokens: value
                    .pointer("/usage/completion_tokens")
                    .and_then(Value::as_u64),
                total_tokens: value.pointer("/usage/total_tokens").and_then(Value::as_u64),
            },
            latency_ms,
        })
    }
}

fn redact(message: &str, api_key: &str) -> String {
    message.replace(api_key, "[REDACTED]")
}

fn truncate(message: &str) -> String {
    message.chars().take(MAX_PROVIDER_ERROR_CHARS).collect()
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (None, None) => None,
        (left, right) => Some(
            left.unwrap_or_default()
                .saturating_add(right.unwrap_or_default()),
        ),
    }
}
