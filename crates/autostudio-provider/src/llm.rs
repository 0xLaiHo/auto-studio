//! Real HTTP-backed LLM inference adapters.

use std::env;
use std::fmt;

use autostudio_core::context::CanonicalMessage;
use autostudio_core::provider::{LlmModelDescriptor, ThinkingControl, ThinkingLevel};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url};
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
    OPENAI_MODELS_PATH, OPENAI_RESPONSES_PATH, PLAN_MAX_OUTPUT_TOKENS, PROTOCOL_ANTHROPIC_MESSAGES,
    PROTOCOL_OPENAI_CHAT_COMPLETIONS, PROTOCOL_OPENAI_RESPONSES, PROVIDER_ANTHROPIC,
    PROVIDER_DEEPSEEK, PROVIDER_KIMI_CODE, PROVIDER_KIMI_OPEN, PROVIDER_OPENAI,
    PROVIDER_REQUEST_TIMEOUT,
};
use crate::stream::{
    InferenceDelta, SseDecoder, SseEvent, StreamingTurnAssembler, anthropic_deltas,
    openai_chat_deltas, openai_responses_deltas,
};
use crate::thinking::{apply_to_request, model_capability};
use crate::{
    AdapterError, InferenceAdapter, InferenceFuture, InferenceOutcome, InferenceProviderDescriptor,
    InferenceTurnRequest, ProviderConfigError,
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
        request: InferenceTurnRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        match self.config.protocol {
            LlmProtocol::OpenAiChatCompletions => self.infer_chat_completions(&request).await,
            LlmProtocol::OpenAiResponses => self.infer_openai_responses(&request).await,
            LlmProtocol::AnthropicMessages => self.infer_anthropic_messages(&request).await,
        }
    }

    async fn infer_chat_completions(
        &self,
        request: &InferenceTurnRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        if request.continuity.is_some() {
            return Err(AdapterError::ContinuityUnavailable(
                "OpenAI-compatible Chat Completions has no private continuity contract".to_owned(),
            ));
        }
        let endpoint = endpoint(&self.config.base_url, CHAT_COMPLETIONS_PATH)?;
        let messages = openai_chat_messages(request);
        let tools = openai_chat_tools(request)?;
        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "required",
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": PLAN_MAX_OUTPUT_TOKENS
        });
        let effective = apply_to_request(
            &mut body,
            &self.config.provider_kind,
            &self.config.model,
            self.config.thinking_level,
        );
        if self.config.provider_kind == PROVIDER_DEEPSEEK && effective.level != ThinkingLevel::Off {
            body["tool_choice"] = Value::String("auto".to_owned());
        }
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(map_transport_error)?;
        let assembled = decode_stream(response, &self.config.api_key, openai_chat_deltas).await?;
        Ok(InferenceOutcome {
            provider: self.descriptor(),
            visible_text: assembled.visible_text,
            tool_calls: assembled.tool_calls,
            usage: assembled.usage,
            response_id: assembled.response_id,
            continuity: assembled.continuity,
        })
    }

    async fn infer_openai_responses(
        &self,
        request: &InferenceTurnRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        let endpoint = endpoint(&self.config.base_url, OPENAI_RESPONSES_PATH)?;
        let input = openai_responses_input(request)?;
        let tools = openai_responses_tools(request)?;
        let mut body = json!({
            "model": self.config.model,
            "instructions": request.prepared.instructions(),
            "input": input,
            "tools": tools,
            "tool_choice": "required",
            "max_output_tokens": PLAN_MAX_OUTPUT_TOKENS,
            "store": false,
            "stream": true
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
        let assembled =
            decode_stream(response, &self.config.api_key, openai_responses_deltas).await?;
        Ok(InferenceOutcome {
            provider: self.descriptor(),
            visible_text: assembled.visible_text,
            tool_calls: assembled.tool_calls,
            usage: assembled.usage,
            response_id: assembled.response_id,
            continuity: assembled.continuity,
        })
    }

    async fn infer_anthropic_messages(
        &self,
        request: &InferenceTurnRequest,
    ) -> Result<InferenceOutcome, AdapterError> {
        let endpoint = endpoint(&self.config.base_url, ANTHROPIC_MESSAGES_PATH)?;
        let messages = anthropic_messages(request)?;
        let tools = anthropic_tools(request)?;
        let mut body = json!({
            "model": self.config.model,
            "max_tokens": PLAN_MAX_OUTPUT_TOKENS,
            "system": request.prepared.instructions(),
            "messages": messages,
            "tools": tools,
            "tool_choice": {"type": "any"},
            "stream": true
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
        let assembled = decode_stream(response, &self.config.api_key, anthropic_deltas).await?;
        Ok(InferenceOutcome {
            provider: self.descriptor(),
            visible_text: assembled.visible_text,
            tool_calls: assembled.tool_calls,
            usage: assembled.usage,
            response_id: assembled.response_id,
            continuity: assembled.continuity,
        })
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

    fn infer(&self, request: InferenceTurnRequest) -> InferenceFuture<'_> {
        Box::pin(async move { self.infer_inner(request).await })
    }
}

fn openai_chat_messages(request: &InferenceTurnRequest) -> Vec<Value> {
    let mut messages = vec![json!({
        "role": "system",
        "content": request.prepared.instructions()
    })];
    for message in request.prepared.messages() {
        match message {
            CanonicalMessage::ContextSummary { content } | CanonicalMessage::User { content } => {
                messages.push(json!({"role": "user", "content": content}));
            }
            CanonicalMessage::Assistant {
                content,
                tool_calls,
            } => {
                let tool_calls = tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.call_id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments_json
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                messages.push(json!({
                    "role": "assistant",
                    "content": content,
                    "tool_calls": tool_calls
                }));
            }
            CanonicalMessage::Tool {
                call_id, content, ..
            } => messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": content
            })),
        }
    }
    messages
}

fn openai_chat_tools(request: &InferenceTurnRequest) -> Result<Vec<Value>, AdapterError> {
    request
        .prepared
        .tools()
        .iter()
        .map(|tool| {
            let parameters = serde_json::from_str::<Value>(&tool.input_schema_json)
                .map_err(|error| invalid_response(&error.to_string()))?;
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": parameters
                }
            }))
        })
        .collect()
}

fn openai_responses_input(request: &InferenceTurnRequest) -> Result<Vec<Value>, AdapterError> {
    let mut input = Vec::new();
    let continuity = continuity_json(
        request,
        crate::continuity::ContinuityFormat::OpenAiResponses,
    )?;
    let mut continuity_used = false;
    for message in request.prepared.messages() {
        match message {
            CanonicalMessage::ContextSummary { content } | CanonicalMessage::User { content } => {
                input.push(json!({"role": "user", "content": content}));
            }
            CanonicalMessage::Assistant {
                content,
                tool_calls,
            } => {
                if !continuity_used
                    && continuity.as_ref().is_some_and(|items| {
                        continuity_matches_tool_calls(items, "function_call", tool_calls)
                    })
                {
                    input.extend(
                        continuity
                            .as_ref()
                            .expect("matched OpenAI Continuity")
                            .iter()
                            .cloned(),
                    );
                    continuity_used = true;
                    continue;
                }
                if let Some(content) = content {
                    input.push(json!({"role": "assistant", "content": content}));
                }
                input.extend(tool_calls.iter().map(|call| {
                    json!({
                        "type": "function_call",
                        "call_id": call.call_id,
                        "name": call.name,
                        "arguments": call.arguments_json
                    })
                }));
            }
            CanonicalMessage::Tool {
                call_id, content, ..
            } => input.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": content
            })),
        }
    }
    if continuity.is_some() && !continuity_used {
        return Err(AdapterError::ContinuityUnavailable(
            "OpenAI Responses state does not match a canonical Tool Request".to_owned(),
        ));
    }
    Ok(input)
}

fn openai_responses_tools(request: &InferenceTurnRequest) -> Result<Vec<Value>, AdapterError> {
    request
        .prepared
        .tools()
        .iter()
        .map(|tool| {
            let parameters = serde_json::from_str::<Value>(&tool.input_schema_json)
                .map_err(|error| invalid_response(&error.to_string()))?;
            Ok(json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": parameters,
                "strict": true
            }))
        })
        .collect()
}

fn anthropic_messages(request: &InferenceTurnRequest) -> Result<Vec<Value>, AdapterError> {
    let mut messages = Vec::new();
    let continuity = continuity_json(
        request,
        crate::continuity::ContinuityFormat::AnthropicMessages,
    )?;
    let mut continuity_used = false;
    for message in request.prepared.messages() {
        match message {
            CanonicalMessage::ContextSummary { content } | CanonicalMessage::User { content } => {
                messages.push(json!({"role": "user", "content": content}));
            }
            CanonicalMessage::Assistant {
                content,
                tool_calls,
            } => {
                if !continuity_used
                    && continuity.as_ref().is_some_and(|blocks| {
                        continuity_matches_tool_calls(blocks, "tool_use", tool_calls)
                    })
                {
                    messages.push(json!({
                        "role": "assistant",
                        "content": continuity.as_ref().expect("matched Anthropic Continuity")
                    }));
                    continuity_used = true;
                    continue;
                }
                let mut blocks = Vec::new();
                if let Some(content) = content {
                    blocks.push(json!({"type": "text", "text": content}));
                }
                for call in tool_calls {
                    let input = serde_json::from_str::<Value>(&call.arguments_json)
                        .map_err(|error| invalid_response(&error.to_string()))?;
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.call_id,
                        "name": call.name,
                        "input": input
                    }));
                }
                messages.push(json!({"role": "assistant", "content": blocks}));
            }
            CanonicalMessage::Tool {
                call_id,
                content,
                is_error,
                ..
            } => messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                    "is_error": is_error
                }]
            })),
        }
    }
    if continuity.is_some() && !continuity_used {
        return Err(AdapterError::ContinuityUnavailable(
            "Anthropic Messages state does not match a canonical Tool Request".to_owned(),
        ));
    }
    Ok(messages)
}

fn continuity_json(
    request: &InferenceTurnRequest,
    expected: crate::continuity::ContinuityFormat,
) -> Result<Option<Vec<Value>>, AdapterError> {
    let Some(state) = &request.continuity else {
        return Ok(None);
    };
    if state.format() != expected {
        return Err(AdapterError::ContinuityUnavailable(format!(
            "state format {:?} cannot be replayed by {:?}",
            state.format(),
            expected
        )));
    }
    let value = state
        .json()
        .map_err(|error| AdapterError::ContinuityUnavailable(error.to_string()))?;
    value.as_array().cloned().map(Some).ok_or_else(|| {
        AdapterError::ContinuityUnavailable("state payload is not an array".to_owned())
    })
}

fn continuity_matches_tool_calls(
    items: &[Value],
    item_type: &str,
    calls: &[autostudio_core::context::CanonicalToolCall],
) -> bool {
    if calls.is_empty() {
        return false;
    }
    let provider_ids = items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some(item_type))
        .filter_map(|item| {
            item.get(if item_type == "tool_use" {
                "id"
            } else {
                "call_id"
            })
            .and_then(Value::as_str)
        })
        .collect::<Vec<_>>();
    calls
        .iter()
        .all(|call| provider_ids.contains(&call.call_id.as_str()))
}

fn anthropic_tools(request: &InferenceTurnRequest) -> Result<Vec<Value>, AdapterError> {
    request
        .prepared
        .tools()
        .iter()
        .map(|tool| {
            let input_schema = serde_json::from_str::<Value>(&tool.input_schema_json)
                .map_err(|error| invalid_response(&error.to_string()))?;
            Ok(json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": input_schema
            }))
        })
        .collect()
}

async fn decode_stream(
    response: reqwest::Response,
    api_key: &str,
    translate: fn(&SseEvent) -> Result<Vec<InferenceDelta>, AdapterError>,
) -> Result<crate::stream::AssembledInferenceTurn, AdapterError> {
    let status = response.status();
    if !status.is_success() {
        let text = response
            .text()
            .await
            .map_err(|error| AdapterError::UnknownOutcome(error.to_string()))?;
        return Err(status_error(status, &text, api_key));
    }
    let mut decoder = SseDecoder::default();
    let mut assembler = StreamingTurnAssembler::default();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_transport_error)?;
        for event in decoder.push(&chunk)? {
            for delta in translate(&event)? {
                assembler.push(delta);
            }
        }
    }
    for event in decoder.finish()? {
        for delta in translate(&event)? {
            assembler.push(delta);
        }
    }
    assembler.finish()
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
    if matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE
    ) && crate::error::is_context_overflow_error(&detail)
    {
        AdapterError::ContextOverflow(message)
    } else if status.is_server_error()
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
    use super::*;

    #[test]
    fn only_explicit_http_context_signals_are_classified_as_overflow() {
        let overflow = status_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"code":"context_length_exceeded","message":"maximum context length exceeded"}}"#,
            "secret",
        );
        assert!(matches!(overflow, AdapterError::ContextOverflow(_)));

        let ordinary_rejection = status_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"code":"invalid_request","message":"tool schema is invalid"}}"#,
            "secret",
        );
        assert!(matches!(ordinary_rejection, AdapterError::Rejected(_)));
    }
}
