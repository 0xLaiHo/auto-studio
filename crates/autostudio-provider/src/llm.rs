//! Real HTTP-backed LLM inference adapters.

use std::env;
use std::fmt;

use autostudio_core::agent::{AgentDecision, CostEstimate, GenerationIntent};
use autostudio_core::provider::{LlmModelDescriptor, ThinkingControl, ThinkingLevel};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::constants::{
    ANTHROPIC_MESSAGES_PATH, ANTHROPIC_MODELS_PATH, ANTHROPIC_VERSION, CHAT_COMPLETIONS_PATH,
    DEFAULT_ANTHROPIC_BASE_URL, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEK_MODEL,
    DEFAULT_KIMI_CODE_BASE_URL, DEFAULT_KIMI_CODE_MODEL, DEFAULT_MOONSHOT_BASE_URL,
    DEFAULT_MOONSHOT_MODEL, DEFAULT_OPENAI_BASE_URL, ENV_ANTHROPIC_API_KEY, ENV_ANTHROPIC_BASE_URL,
    ENV_ANTHROPIC_MODEL, ENV_DEEPSEEK_API_KEY, ENV_DEEPSEEK_BASE_URL, ENV_DEEPSEEK_MODEL,
    ENV_KIMI_CODE_API_KEY, ENV_KIMI_CODE_BASE_URL, ENV_KIMI_CODE_MODEL, ENV_MOONSHOT_API_KEY,
    ENV_MOONSHOT_BASE_URL, ENV_MOONSHOT_MODEL, ENV_OPENAI_API_KEY, ENV_OPENAI_BASE_URL,
    ENV_OPENAI_MODEL, KIMI_CODE_CATALOG_SNAPSHOT_DATE, MAX_PROVIDER_ERROR_CHARS,
    OPENAI_MODELS_PATH, OPENAI_RESPONSES_PATH, PLAN_MAX_OUTPUT_TOKENS, PLAN_SYSTEM_PROMPT,
    PLAN_TOOL_NAME, PROTOCOL_ANTHROPIC_MESSAGES, PROTOCOL_OPENAI_CHAT_COMPLETIONS,
    PROTOCOL_OPENAI_RESPONSES, PROVIDER_ANTHROPIC, PROVIDER_DEEPSEEK, PROVIDER_KIMI_CODE,
    PROVIDER_KIMI_OPEN, PROVIDER_OPENAI, PROVIDER_REQUEST_TIMEOUT,
};
use crate::thinking::{apply_to_request, model_capability};
use crate::{
    AdapterError, InferenceAdapter, InferenceFuture, InferenceOutcome, InferenceProviderDescriptor,
    InferenceRequest, ProviderConfigError, Usage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LlmProtocol {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
}

#[derive(Clone)]
pub struct LlmProviderConnection {
    provider_kind: String,
    protocol: LlmProtocol,
    base_url: Url,
    api_key: String,
}

impl fmt::Debug for LlmProviderConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmProviderConnection")
            .field("provider_kind", &self.provider_kind)
            .field("protocol", &self.protocol)
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl Drop for LlmProviderConnection {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

impl LlmProviderConnection {
    /// Validates Provider credentials and endpoint metadata without requiring
    /// a model selection.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError`] for an unsupported Provider, empty key,
    /// or invalid base URL.
    pub fn from_connection(
        provider: &str,
        api_key: impl Into<String>,
        base_url: Option<&str>,
    ) -> Result<Self, ProviderConfigError> {
        let provider = provider.trim().to_ascii_lowercase();
        let (protocol, default_base) = provider_metadata(&provider)?;
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ProviderConfigError::MissingSetting {
                provider,
                variable: "api_key",
            });
        }
        let base_url = parse_base_url(&provider, "base_url", base_url.unwrap_or(default_base))?;
        Ok(Self {
            provider_kind: provider,
            protocol,
            base_url,
            api_key,
        })
    }

    /// Fetches the models visible to the configured credential.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when the Provider rejects the credential,
    /// cannot be reached, or returns an invalid catalog response.
    pub async fn list_models(&self) -> Result<Vec<LlmModelDescriptor>, AdapterError> {
        if self.provider_kind == PROVIDER_KIMI_CODE {
            return Ok([
                "k3",
                "k3-256k",
                "kimi-for-coding",
                "kimi-for-coding-highspeed",
            ]
            .into_iter()
            .map(|id| {
                model_descriptor(
                    &self.provider_kind,
                    id,
                    &format!("{id} · official snapshot {KIMI_CODE_CATALOG_SNAPSHOT_DATE}"),
                )
            })
            .collect());
        }
        let client = Client::builder()
            .timeout(PROVIDER_REQUEST_TIMEOUT)
            .build()
            .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
        let path = if self.protocol == LlmProtocol::AnthropicMessages {
            ANTHROPIC_MODELS_PATH
        } else {
            OPENAI_MODELS_PATH
        };
        let endpoint = endpoint(&self.base_url, path)?;
        let request = client.get(endpoint);
        let request = if self.protocol == LlmProtocol::AnthropicMessages {
            request
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
        } else {
            request.bearer_auth(&self.api_key)
        };
        let response = request
            .send()
            .await
            .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
        let value = decode_response(response, &self.api_key).await?;
        let items = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_response("model catalog is missing data"))?;
        let mut models = items
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.trim();
                if id.is_empty() {
                    return None;
                }
                let display_name = item
                    .get("display_name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or(id);
                Some(model_descriptor(&self.provider_kind, id, display_name))
            })
            .collect::<Vec<_>>();
        models.sort_by(|left, right| left.id.cmp(&right.id));
        models.dedup_by(|left, right| left.id == right.id);
        if models.is_empty() {
            return Err(invalid_response(
                "model catalog contains no usable model ids",
            ));
        }
        Ok(models)
    }

    #[must_use]
    pub fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Binds a validated model ID to this connection.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError`] when the model ID is empty.
    pub fn with_model(
        &self,
        model: impl Into<String>,
    ) -> Result<LlmProviderConfig, ProviderConfigError> {
        LlmProviderConfig::new(
            self.provider_kind.clone(),
            self.protocol,
            self.base_url.as_str(),
            model,
            self.api_key.clone(),
        )
    }
}

impl LlmProtocol {
    const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => PROTOCOL_OPENAI_CHAT_COMPLETIONS,
            Self::OpenAiResponses => PROTOCOL_OPENAI_RESPONSES,
            Self::AnthropicMessages => PROTOCOL_ANTHROPIC_MESSAGES,
        }
    }
}

#[derive(Clone)]
pub struct LlmProviderConfig {
    provider_kind: String,
    protocol: LlmProtocol,
    base_url: Url,
    model: String,
    thinking_level: ThinkingLevel,
    api_key: String,
}

impl fmt::Debug for LlmProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmProviderConfig")
            .field("provider_kind", &self.provider_kind)
            .field("protocol", &self.protocol)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("thinking_level", &self.thinking_level)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl Drop for LlmProviderConfig {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

impl LlmProviderConfig {
    /// Resolves one explicitly selected Provider from environment variables.
    /// Secrets remain process-local and are never placed in Project state.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError`] for an unsupported Provider, missing
    /// credential/model, or invalid/insecure base URL.
    pub fn from_environment(provider: &str) -> Result<Self, ProviderConfigError> {
        match provider.trim().to_ascii_lowercase().as_str() {
            PROVIDER_DEEPSEEK => Self::from_environment_parts(
                PROVIDER_DEEPSEEK,
                LlmProtocol::OpenAiChatCompletions,
                ENV_DEEPSEEK_API_KEY,
                ENV_DEEPSEEK_BASE_URL,
                DEFAULT_DEEPSEEK_BASE_URL,
                ENV_DEEPSEEK_MODEL,
                Some(DEFAULT_DEEPSEEK_MODEL),
            ),
            PROVIDER_OPENAI => Self::from_environment_parts(
                PROVIDER_OPENAI,
                LlmProtocol::OpenAiResponses,
                ENV_OPENAI_API_KEY,
                ENV_OPENAI_BASE_URL,
                DEFAULT_OPENAI_BASE_URL,
                ENV_OPENAI_MODEL,
                None,
            ),
            PROVIDER_ANTHROPIC => Self::from_environment_parts(
                PROVIDER_ANTHROPIC,
                LlmProtocol::AnthropicMessages,
                ENV_ANTHROPIC_API_KEY,
                ENV_ANTHROPIC_BASE_URL,
                DEFAULT_ANTHROPIC_BASE_URL,
                ENV_ANTHROPIC_MODEL,
                None,
            ),
            PROVIDER_KIMI_OPEN => Self::from_environment_parts(
                PROVIDER_KIMI_OPEN,
                LlmProtocol::OpenAiChatCompletions,
                ENV_MOONSHOT_API_KEY,
                ENV_MOONSHOT_BASE_URL,
                DEFAULT_MOONSHOT_BASE_URL,
                ENV_MOONSHOT_MODEL,
                Some(DEFAULT_MOONSHOT_MODEL),
            ),
            PROVIDER_KIMI_CODE => Self::from_environment_parts(
                PROVIDER_KIMI_CODE,
                LlmProtocol::AnthropicMessages,
                ENV_KIMI_CODE_API_KEY,
                ENV_KIMI_CODE_BASE_URL,
                DEFAULT_KIMI_CODE_BASE_URL,
                ENV_KIMI_CODE_MODEL,
                Some(DEFAULT_KIMI_CODE_MODEL),
            ),
            other => Err(ProviderConfigError::UnsupportedProvider(other.to_owned())),
        }
    }

    /// Constructs a connection explicitly. This is intended for local contract
    /// tests and callers that resolve secrets from an OS credential vault.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError`] when a required value is empty or the
    /// base URL is invalid/insecure.
    pub fn new(
        provider_kind: impl Into<String>,
        protocol: LlmProtocol,
        base_url: &str,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, ProviderConfigError> {
        let provider_kind = provider_kind.into();
        let model = model.into();
        let api_key = api_key.into();
        if model.trim().is_empty() {
            return Err(ProviderConfigError::MissingSetting {
                provider: provider_kind,
                variable: "model",
            });
        }
        if api_key.trim().is_empty() {
            return Err(ProviderConfigError::MissingSetting {
                provider: provider_kind,
                variable: "api_key",
            });
        }
        let base_url = parse_base_url(&provider_kind, "base_url", base_url)?;
        Ok(Self {
            provider_kind,
            protocol,
            base_url,
            model,
            thinking_level: ThinkingLevel::default(),
            api_key,
        })
    }

    /// Resolves a Provider Connection submitted through the local Core.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError`] for an unsupported Provider, a missing
    /// model with no safe default, an empty credential, or an invalid base URL.
    pub fn from_connection(
        provider: &str,
        api_key: impl Into<String>,
        model: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<Self, ProviderConfigError> {
        let provider = provider.trim().to_ascii_lowercase();
        let default_model = match provider.as_str() {
            PROVIDER_DEEPSEEK => Some(DEFAULT_DEEPSEEK_MODEL),
            PROVIDER_OPENAI | PROVIDER_ANTHROPIC => None,
            PROVIDER_KIMI_OPEN => Some(DEFAULT_MOONSHOT_MODEL),
            PROVIDER_KIMI_CODE => Some(DEFAULT_KIMI_CODE_MODEL),
            _ => return Err(ProviderConfigError::UnsupportedProvider(provider)),
        };
        let model = model
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| default_model.map(str::to_owned))
            .ok_or_else(|| ProviderConfigError::MissingSetting {
                provider: provider.clone(),
                variable: "model",
            })?;
        LlmProviderConnection::from_connection(&provider, api_key, base_url)?.with_model(model)
    }

    #[must_use]
    pub fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn thinking_level(&self) -> ThinkingLevel {
        self.thinking_level
    }

    #[must_use]
    pub fn with_thinking_level(mut self, thinking_level: ThinkingLevel) -> Self {
        self.thinking_level = thinking_level;
        self
    }

    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn from_environment_parts(
        provider: &'static str,
        protocol: LlmProtocol,
        key_variable: &'static str,
        base_variable: &'static str,
        default_base: &'static str,
        model_variable: &'static str,
        default_model: Option<&'static str>,
    ) -> Result<Self, ProviderConfigError> {
        let api_key = required_environment(provider, key_variable)?;
        let model = match env::var(model_variable) {
            Ok(value) if !value.trim().is_empty() => value,
            _ => default_model.map(str::to_owned).ok_or_else(|| {
                ProviderConfigError::MissingSetting {
                    provider: provider.to_owned(),
                    variable: model_variable,
                }
            })?,
        };
        let base = env::var(base_variable).unwrap_or_else(|_| default_base.to_owned());
        let base_url = parse_base_url(provider, base_variable, &base)?;
        Ok(Self {
            provider_kind: provider.to_owned(),
            protocol,
            base_url,
            model,
            thinking_level: ThinkingLevel::default(),
            api_key,
        })
    }
}

pub struct HttpInferenceAdapter {
    client: Client,
    config: LlmProviderConfig,
}

impl HttpInferenceAdapter {
    /// Creates the real Provider adapter.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError`] if the HTTP client cannot be built.
    pub fn new(config: LlmProviderConfig) -> Result<Self, ProviderConfigError> {
        let client = Client::builder()
            .timeout(PROVIDER_REQUEST_TIMEOUT)
            .build()
            .map_err(|error| ProviderConfigError::HttpClient(error.to_string()))?;
        Ok(Self { client, config })
    }

    async fn infer_inner(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        match self.config.protocol {
            LlmProtocol::OpenAiChatCompletions => self.infer_chat_completions(&request).await,
            LlmProtocol::OpenAiResponses => self.infer_openai_responses(&request).await,
            LlmProtocol::AnthropicMessages => self.infer_anthropic_messages(&request).await,
        }
    }

    async fn infer_chat_completions(
        &self,
        request: &InferenceRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        let endpoint = endpoint(&self.config.base_url, CHAT_COMPLETIONS_PATH)?;
        let mut body = json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": PLAN_SYSTEM_PROMPT},
                {"role": "user", "content": brief_prompt(request)?}
            ],
            "response_format": {"type": "json_object"},
            "stream": false,
            "max_tokens": PLAN_MAX_OUTPUT_TOKENS
        });
        apply_to_request(
            &mut body,
            &self.config.provider_kind,
            &self.config.model,
            self.config.thinking_level,
        );
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(map_transport_error)?;
        let value = decode_response(response, &self.config.api_key).await?;
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_response("missing choices[0].message.content"))?;
        let output = parse_plan_output(content)?;
        Ok(output.into_inference_outcome(
            self.descriptor(),
            usage_from_chat(&value),
            value.get("id").and_then(Value::as_str).map(str::to_owned),
        ))
    }

    async fn infer_openai_responses(
        &self,
        request: &InferenceRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        let endpoint = endpoint(&self.config.base_url, OPENAI_RESPONSES_PATH)?;
        let mut body = json!({
            "model": self.config.model,
            "instructions": PLAN_SYSTEM_PROMPT,
            "input": brief_prompt(request)?,
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "creative_plan",
                    "strict": true,
                    "schema": plan_schema()
                }
            },
            "max_output_tokens": PLAN_MAX_OUTPUT_TOKENS,
            "store": false
        });
        apply_to_request(
            &mut body,
            &self.config.provider_kind,
            &self.config.model,
            self.config.thinking_level,
        );
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(map_transport_error)?;
        let value = decode_response(response, &self.config.api_key).await?;
        let content = openai_response_text(&value)
            .ok_or_else(|| invalid_response("missing Responses API output text"))?;
        let output = parse_plan_output(&content)?;
        let usage = Usage {
            input_tokens: value.pointer("/usage/input_tokens").and_then(Value::as_u64),
            output_tokens: value
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64),
            actual_cost_minor_units: None,
            currency: None,
        };
        Ok(output.into_inference_outcome(
            self.descriptor(),
            usage,
            value.get("id").and_then(Value::as_str).map(str::to_owned),
        ))
    }

    async fn infer_anthropic_messages(
        &self,
        request: &InferenceRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        let endpoint = endpoint(&self.config.base_url, ANTHROPIC_MESSAGES_PATH)?;
        let mut body = json!({
            "model": self.config.model,
            "max_tokens": PLAN_MAX_OUTPUT_TOKENS,
            "system": PLAN_SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": brief_prompt(request)?}],
            "tools": [{
                "name": PLAN_TOOL_NAME,
                "description": "Submit the creator-visible music generation plan",
                "input_schema": plan_schema()
            }],
            "tool_choice": {"type": "tool", "name": PLAN_TOOL_NAME}
        });
        let effective = apply_to_request(
            &mut body,
            &self.config.provider_kind,
            &self.config.model,
            self.config.thinking_level,
        );
        if let Some(budget) = effective.budget_tokens {
            body["max_tokens"] = Value::from(budget.saturating_add(PLAN_MAX_OUTPUT_TOKENS));
        }
        if effective.control == ThinkingControl::TokenBudget {
            body["tool_choice"] = json!({"type": "auto"});
        }
        let response = self
            .client
            .post(endpoint)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(map_transport_error)?;
        let value = decode_response(response, &self.config.api_key).await?;
        let input = value
            .get("content")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("type").and_then(Value::as_str) == Some("tool_use")
                        && item.get("name").and_then(Value::as_str) == Some(PLAN_TOOL_NAME)
                })
            })
            .and_then(|item| item.get("input"))
            .ok_or_else(|| invalid_response("missing required Anthropic plan tool result"))?;
        let output: PlanOutput = serde_json::from_value(input.clone())
            .map_err(|error| invalid_response(&error.to_string()))?;
        let usage = Usage {
            input_tokens: value.pointer("/usage/input_tokens").and_then(Value::as_u64),
            output_tokens: value
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64),
            actual_cost_minor_units: None,
            currency: None,
        };
        Ok(output.into_inference_outcome(
            self.descriptor(),
            usage,
            value.get("id").and_then(Value::as_str).map(str::to_owned),
        ))
    }
}

impl InferenceAdapter for HttpInferenceAdapter {
    fn descriptor(&self) -> InferenceProviderDescriptor {
        let mut request = json!({});
        let effective = apply_to_request(
            &mut request,
            &self.config.provider_kind,
            &self.config.model,
            self.config.thinking_level,
        );
        InferenceProviderDescriptor {
            provider_kind: self.config.provider_kind.clone(),
            model: self.config.model.clone(),
            thinking_level: effective.level,
            thinking_control: effective.control,
            thinking_budget_tokens: effective.budget_tokens,
            capability_revision: effective.capability_revision.to_owned(),
            mapping_revision: effective.mapping_revision.to_owned(),
            protocol: self.config.protocol.as_str().to_owned(),
        }
    }

    fn infer(&self, request: InferenceRequest) -> InferenceFuture<'_> {
        Box::pin(async move { self.infer_inner(request).await })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanOutput {
    visible_summary: String,
    generation_prompt: String,
    duration_seconds: u32,
    candidate_count: u8,
}

impl PlanOutput {
    fn into_inference_outcome(
        self,
        provider: InferenceProviderDescriptor,
        usage: Usage,
        response_id: Option<String>,
    ) -> InferenceOutcome {
        InferenceOutcome {
            provider,
            visible_summary: self.visible_summary,
            decision: AgentDecision::GenerateMusic(GenerationIntent {
                prompt: self.generation_prompt,
                duration_seconds: self.duration_seconds,
                candidate_count: self.candidate_count,
            }),
            estimated_cost: CostEstimate::Unknown,
            usage,
            response_id,
        }
    }
}

fn plan_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "visibleSummary": {"type": "string", "minLength": 1},
            "generationPrompt": {"type": "string", "minLength": 1},
            "durationSeconds": {"type": "integer", "minimum": 1, "maximum": 900},
            "candidateCount": {"type": "integer", "minimum": 1, "maximum": 4}
        },
        "required": [
            "visibleSummary",
            "generationPrompt",
            "durationSeconds",
            "candidateCount"
        ]
    })
}

fn brief_prompt(request: &InferenceRequest) -> Result<String, AdapterError> {
    let brief = serde_json::to_string(&request.brief)
        .map_err(|error| invalid_response(&error.to_string()))?;
    Ok(format!(
        "Project context revision: {}\nCreative Brief JSON: {brief}\nReturn the requested plan JSON.",
        request.context_revision
    ))
}

fn parse_plan_output(content: &str) -> Result<PlanOutput, AdapterError> {
    let content = strip_json_fence(content.trim());
    serde_json::from_str(content).map_err(|error| invalid_response(&error.to_string()))
}

fn strip_json_fence(value: &str) -> &str {
    let Some(rest) = value.strip_prefix("```") else {
        return value;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    rest.trim_start_matches([' ', '\t', '\r', '\n'])
        .strip_suffix("```")
        .map_or(value, str::trim_end)
}

fn usage_from_chat(value: &Value) -> Usage {
    Usage {
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64),
        actual_cost_minor_units: None,
        currency: None,
    }
}

fn openai_response_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        return Some(text.to_owned());
    }
    let output = value.get("output")?.as_array()?;
    let mut text = output
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .peekable();
    text.peek()?;
    Some(text.collect())
}

async fn decode_response(
    response: reqwest::Response,
    api_key: &str,
) -> Result<Value, AdapterError> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| AdapterError::UnknownOutcome(error.to_string()))?;
    if !status.is_success() {
        return Err(status_error(status, &text, api_key));
    }
    serde_json::from_str(&text).map_err(|error| invalid_response(&error.to_string()))
}

fn status_error(status: StatusCode, body: &str, api_key: &str) -> AdapterError {
    let detail = sanitized_error_detail(body, api_key);
    let message = format!("HTTP {}: {detail}", status.as_u16());
    if status.is_server_error()
        || matches!(
            status,
            StatusCode::REQUEST_TIMEOUT | StatusCode::CONFLICT | StatusCode::TOO_MANY_REQUESTS
        )
    {
        AdapterError::Unavailable(message)
    } else {
        AdapterError::Rejected(message)
    }
}

fn sanitized_error_detail(body: &str, api_key: &str) -> String {
    let redacted = body.replace(api_key, "[REDACTED]");
    let normalized: String = redacted
        .chars()
        .filter(|character| !character.is_control() || *character == ' ')
        .take(MAX_PROVIDER_ERROR_CHARS)
        .collect();
    if normalized.trim().is_empty() {
        "Provider returned no error detail".to_owned()
    } else {
        normalized
    }
}

fn map_transport_error(error: reqwest::Error) -> AdapterError {
    let is_connect = error.is_connect();
    let message = error.to_string();
    drop(error);
    if is_connect {
        AdapterError::Unavailable(message)
    } else {
        AdapterError::UnknownOutcome(format!(
            "LLM request may have consumed tokens before the connection failed: {message}"
        ))
    }
}

fn provider_metadata(provider: &str) -> Result<(LlmProtocol, &'static str), ProviderConfigError> {
    match provider {
        PROVIDER_DEEPSEEK => Ok((
            LlmProtocol::OpenAiChatCompletions,
            DEFAULT_DEEPSEEK_BASE_URL,
        )),
        PROVIDER_OPENAI => Ok((LlmProtocol::OpenAiResponses, DEFAULT_OPENAI_BASE_URL)),
        PROVIDER_ANTHROPIC => Ok((LlmProtocol::AnthropicMessages, DEFAULT_ANTHROPIC_BASE_URL)),
        PROVIDER_KIMI_OPEN => Ok((
            LlmProtocol::OpenAiChatCompletions,
            DEFAULT_MOONSHOT_BASE_URL,
        )),
        PROVIDER_KIMI_CODE => Ok((LlmProtocol::AnthropicMessages, DEFAULT_KIMI_CODE_BASE_URL)),
        _ => Err(ProviderConfigError::UnsupportedProvider(
            provider.to_owned(),
        )),
    }
}

fn model_descriptor(provider: &str, id: &str, display_name: &str) -> LlmModelDescriptor {
    LlmModelDescriptor {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        thinking: model_capability(provider, id),
    }
}

fn endpoint(base_url: &Url, path: &str) -> Result<Url, AdapterError> {
    base_url
        .join(path)
        .map_err(|error| AdapterError::Unavailable(error.to_string()))
}

fn required_environment(
    provider: &str,
    variable: &'static str,
) -> Result<String, ProviderConfigError> {
    env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ProviderConfigError::MissingSetting {
            provider: provider.to_owned(),
            variable,
        })
}

fn parse_base_url(
    provider: &str,
    variable: &'static str,
    value: &str,
) -> Result<Url, ProviderConfigError> {
    let normalized = format!("{}/", value.trim().trim_end_matches('/'));
    let url = Url::parse(&normalized).map_err(|_| ProviderConfigError::InvalidBaseUrl {
        provider: provider.to_owned(),
        variable,
    })?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(ProviderConfigError::InvalidBaseUrl {
            provider: provider.to_owned(),
            variable,
        });
    }
    Ok(url)
}

fn invalid_response(message: &str) -> AdapterError {
    AdapterError::InvalidResponse(message.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{openai_response_text, parse_plan_output, strip_json_fence};

    #[test]
    fn parses_fenced_plan_json_without_accepting_prose() {
        let output = parse_plan_output(
            "```json\n{\"visibleSummary\":\"A/B\",\"generationPrompt\":\"warm piano\",\"durationSeconds\":30,\"candidateCount\":2}\n```",
        )
        .expect("valid plan");
        assert_eq!(output.candidate_count, 2);
        assert_eq!(strip_json_fence("not fenced"), "not fenced");
    }

    #[test]
    fn extracts_nested_openai_responses_text() {
        let response = json!({
            "output": [{
                "content": [{"type": "output_text", "text": "first"}, {"type": "output_text", "text": "second"}]
            }]
        });
        assert_eq!(
            openai_response_text(&response).as_deref(),
            Some("firstsecond")
        );
    }
}
