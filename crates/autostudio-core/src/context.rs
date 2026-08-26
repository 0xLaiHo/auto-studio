//! Canonical, Provider-independent Agent context and transcript vocabulary.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::{AgentRunId, InferenceUsage};
pub use crate::error::{ContextError, ContextStoreError};
use crate::provider::{ThinkingControl, ThinkingLevel};

macro_rules! context_id {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Parses an identity received from durable storage or transport.
            ///
            /// # Errors
            ///
            /// Returns [`ContextError::InvalidId`] for malformed input.
            pub fn parse(value: &str) -> Result<Self, ContextError> {
                Uuid::parse_str(value)
                    .map(Self)
                    .map_err(|_| ContextError::InvalidId($label))
            }

            #[must_use]
            pub fn as_str(&self) -> String {
                self.0.to_string()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

context_id!(ContextId, "Context Snapshot");
context_id!(InferenceTurnId, "Inference Turn");
context_id!(InferenceItemId, "Inference Item");

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibleMessageRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum CanonicalMessage {
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<CanonicalToolCall>,
    },
    Tool {
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
    pub descriptor_fingerprint: String,
}

impl CanonicalToolDefinition {
    /// Creates a model-visible Tool definition after validating its stable fields.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when a field is empty, the schema is not JSON,
    /// or the fingerprint is not a SHA-256 digest.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema_json: impl Into<String>,
        descriptor_fingerprint: impl Into<String>,
    ) -> Result<Self, ContextError> {
        let definition = Self {
            name: name.into(),
            description: description.into(),
            input_schema_json: input_schema_json.into(),
            descriptor_fingerprint: descriptor_fingerprint.into(),
        };
        definition.validate()?;
        Ok(definition)
    }

    /// Revalidates a Tool definition restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for empty fields, invalid JSON, or fingerprint.
    pub fn validate(&self) -> Result<(), ContextError> {
        require_text(&self.name, "tool.name")?;
        require_text(&self.description, "tool.description")?;
        serde_json::from_str::<serde_json::Value>(&self.input_schema_json)
            .map_err(|_| ContextError::InvalidJson("tool.input_schema_json"))?;
        require_digest(&self.descriptor_fingerprint)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceFinishReason {
    Completed,
    ProviderRejected,
    ProviderUnavailable,
    InvalidResponse,
    Interrupted,
    UnknownConsumption,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InferenceItemDraft {
    VisibleMessage {
        role: VisibleMessageRole,
        content: String,
    },
    ToolRequest {
        call_id: String,
        name: String,
        arguments_json: String,
        descriptor_fingerprint: String,
    },
    ToolResult {
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
        execution_id: Option<String>,
    },
    Usage {
        usage: InferenceUsage,
    },
    Finish {
        reason: InferenceFinishReason,
        detail: Option<String>,
    },
}

impl InferenceItemDraft {
    /// Revalidates a payload restored from untrusted durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when required text, JSON, or fingerprints are invalid.
    pub fn validate(&self) -> Result<(), ContextError> {
        match self {
            Self::VisibleMessage { content, .. } => require_text(content, "message.content"),
            Self::ToolRequest {
                call_id,
                name,
                arguments_json,
                descriptor_fingerprint,
            } => {
                require_text(call_id, "tool_request.call_id")?;
                require_text(name, "tool_request.name")?;
                serde_json::from_str::<serde_json::Value>(arguments_json)
                    .map_err(|_| ContextError::InvalidJson("tool_request.arguments_json"))?;
                require_digest(descriptor_fingerprint)
            }
            Self::ToolResult {
                call_id,
                name,
                content,
                execution_id,
                ..
            } => {
                require_text(call_id, "tool_result.call_id")?;
                require_text(name, "tool_result.name")?;
                require_text(content, "tool_result.content")?;
                if let Some(execution_id) = execution_id {
                    require_text(execution_id, "tool_result.execution_id")?;
                }
                Ok(())
            }
            Self::Usage { .. } => Ok(()),
            Self::Finish { detail, .. } => {
                if let Some(detail) = detail {
                    require_text(detail, "finish.detail")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceItem {
    id: InferenceItemId,
    run_id: AgentRunId,
    turn_id: InferenceTurnId,
    sequence: u64,
    created_at_unix_millis: u64,
    content_hash: String,
    payload: InferenceItemDraft,
}

impl InferenceItem {
    /// Creates a validated, complete transcript item. Partial streaming data
    /// must not cross this constructor.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for invalid payloads, sequence zero, or digest.
    pub fn new(
        run_id: AgentRunId,
        turn_id: InferenceTurnId,
        sequence: u64,
        created_at_unix_millis: u64,
        content_hash: String,
        payload: InferenceItemDraft,
    ) -> Result<Self, ContextError> {
        if sequence == 0 {
            return Err(ContextError::InconsistentJournal(
                "Inference Item sequence must start at one".to_owned(),
            ));
        }
        require_digest(&content_hash)?;
        payload.validate()?;
        Ok(Self {
            id: InferenceItemId::new(),
            run_id,
            turn_id,
            sequence,
            created_at_unix_millis,
            content_hash,
            payload,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &InferenceItemId {
        &self.id
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &InferenceTurnId {
        &self.turn_id
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn payload(&self) -> &InferenceItemDraft {
        &self.payload
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBinding {
    pub provider_kind: String,
    pub model: String,
    pub protocol: String,
    #[serde(rename = "modelEffort")]
    pub thinking_level: ThinkingLevel,
    pub thinking_control: ThinkingControl,
    pub thinking_budget_tokens: Option<u32>,
    pub capability_revision: String,
    pub mapping_revision: String,
    pub tool_catalog_fingerprint: String,
}

impl ProviderBinding {
    /// Validates an exact Provider chain binding.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for empty identity fields or an invalid digest.
    pub fn validate(&self) -> Result<(), ContextError> {
        require_text(&self.provider_kind, "provider.provider_kind")?;
        require_text(&self.model, "provider.model")?;
        require_text(&self.protocol, "provider.protocol")?;
        require_text(&self.capability_revision, "provider.capability_revision")?;
        require_text(&self.mapping_revision, "provider.mapping_revision")?;
        require_digest(&self.tool_catalog_fingerprint)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBudgetPlan {
    pub context_window_tokens: Option<u64>,
    pub output_reserve_tokens: u64,
    pub safety_margin_tokens: u64,
    pub input_budget_tokens: Option<u64>,
}

impl TokenBudgetPlan {
    /// Creates a budget for a known model window.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError::InvalidTokenBudget`] when reserves exhaust the window.
    pub fn known(
        context_window_tokens: u64,
        output_reserve_tokens: u64,
        safety_margin_tokens: u64,
    ) -> Result<Self, ContextError> {
        let reserved = output_reserve_tokens
            .checked_add(safety_margin_tokens)
            .ok_or(ContextError::InvalidTokenBudget)?;
        let input_budget_tokens = context_window_tokens
            .checked_sub(reserved)
            .filter(|value| *value > 0)
            .ok_or(ContextError::InvalidTokenBudget)?;
        Ok(Self {
            context_window_tokens: Some(context_window_tokens),
            output_reserve_tokens,
            safety_margin_tokens,
            input_budget_tokens: Some(input_budget_tokens),
        })
    }

    #[must_use]
    pub const fn unknown(output_reserve_tokens: u64, safety_margin_tokens: u64) -> Self {
        Self {
            context_window_tokens: None,
            output_reserve_tokens,
            safety_margin_tokens,
            input_budget_tokens: None,
        }
    }

    /// Revalidates a token budget restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError::InvalidTokenBudget`] for inconsistent fields.
    pub fn validate(&self) -> Result<(), ContextError> {
        match (self.context_window_tokens, self.input_budget_tokens) {
            (None, None) => Ok(()),
            (Some(window), Some(input)) => {
                let reserved = self
                    .output_reserve_tokens
                    .checked_add(self.safety_margin_tokens)
                    .ok_or(ContextError::InvalidTokenBudget)?;
                if window.checked_sub(reserved) == Some(input) && input > 0 {
                    Ok(())
                } else {
                    Err(ContextError::InvalidTokenBudget)
                }
            }
            (Some(_), None) | (None, Some(_)) => Err(ContextError::InvalidTokenBudget),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextManifest {
    context_id: ContextId,
    run_id: AgentRunId,
    turn_id: InferenceTurnId,
    project_id: String,
    project_revision: u64,
    source_log_revision: u64,
    transcript_revision: u64,
    included_item_ids: Vec<InferenceItemId>,
    instructions: String,
    tools: Vec<CanonicalToolDefinition>,
    provider_binding: ProviderBinding,
    token_budget: TokenBudgetPlan,
    content_hash: String,
}

impl ContextManifest {
    #[allow(clippy::too_many_arguments)]
    /// Creates the immutable record of what one Inference Turn can see.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when a binding, project identity, or digest is invalid.
    pub fn new(
        run_id: AgentRunId,
        turn_id: InferenceTurnId,
        project_id: String,
        project_revision: u64,
        source_log_revision: u64,
        transcript_revision: u64,
        included_item_ids: Vec<InferenceItemId>,
        instructions: String,
        tools: Vec<CanonicalToolDefinition>,
        provider_binding: ProviderBinding,
        token_budget: TokenBudgetPlan,
        content_hash: String,
    ) -> Result<Self, ContextError> {
        let manifest = Self {
            context_id: ContextId::new(),
            run_id,
            turn_id,
            project_id,
            project_revision,
            source_log_revision,
            transcript_revision,
            included_item_ids,
            instructions,
            tools,
            provider_binding,
            token_budget,
            content_hash,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    #[must_use]
    pub const fn context_id(&self) -> &ContextId {
        &self.context_id
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &InferenceTurnId {
        &self.turn_id
    }

    #[must_use]
    pub const fn transcript_revision(&self) -> u64 {
        self.transcript_revision
    }

    #[must_use]
    pub const fn provider_binding(&self) -> &ProviderBinding {
        &self.provider_binding
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn source_log_revision(&self) -> u64 {
        self.source_log_revision
    }

    #[must_use]
    pub fn included_item_ids(&self) -> &[InferenceItemId] {
        &self.included_item_ids
    }

    #[must_use]
    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    #[must_use]
    pub fn tools(&self) -> &[CanonicalToolDefinition] {
        &self.tools
    }

    #[must_use]
    pub const fn token_budget(&self) -> &TokenBudgetPlan {
        &self.token_budget
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// Revalidates a Context Manifest restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when required identity, budget, Tool, or digest
    /// fields are inconsistent.
    pub fn validate(&self) -> Result<(), ContextError> {
        require_text(&self.project_id, "manifest.project_id")?;
        require_text(&self.instructions, "manifest.instructions")?;
        if self.included_item_ids.is_empty() {
            return Err(ContextError::InconsistentJournal(
                "Context Manifest must include at least one transcript item".to_owned(),
            ));
        }
        for tool in &self.tools {
            tool.validate()?;
        }
        self.provider_binding.validate()?;
        self.token_budget.validate()?;
        require_digest(&self.content_hash)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextEvent {
    InferenceItemAppended { item: InferenceItem },
    ContextPrepared { manifest: Box<ContextManifest> },
}

impl ContextEvent {
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::InferenceItemAppended { .. } => "inference_item.appended",
            Self::ContextPrepared { .. } => "context.prepared",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEventEnvelope {
    sequence: u64,
    event: ContextEvent,
}

impl ContextEventEnvelope {
    #[must_use]
    pub const fn new(sequence: u64, event: ContextEvent) -> Self {
        Self { sequence, event }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn event(&self) -> &ContextEvent {
        &self.event
    }
}

/// Persistence seam for a Run-scoped, append-only context journal.
pub trait ContextEventStore: Send + Sync {
    /// Atomically appends a batch when the caller still owns the journal revision.
    ///
    /// # Errors
    ///
    /// Returns [`ContextStoreError`] for conflicts, corruption, or unavailable storage.
    fn append_context_events(
        &self,
        run_id: &AgentRunId,
        expected_revision: u64,
        events: &[ContextEvent],
    ) -> Result<u64, ContextStoreError>;

    /// Returns the complete durable journal for one Run in sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`ContextStoreError`] for corrupt or unavailable storage.
    fn context_events(
        &self,
        run_id: &AgentRunId,
    ) -> Result<Vec<ContextEventEnvelope>, ContextStoreError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedContext {
    manifest: ContextManifest,
    messages: Vec<CanonicalMessage>,
    journal_revision: u64,
}

impl PreparedContext {
    #[must_use]
    pub fn new(
        manifest: ContextManifest,
        messages: Vec<CanonicalMessage>,
        journal_revision: u64,
    ) -> Self {
        Self {
            manifest,
            messages,
            journal_revision,
        }
    }

    #[must_use]
    pub const fn manifest(&self) -> &ContextManifest {
        &self.manifest
    }

    #[must_use]
    pub fn instructions(&self) -> &str {
        self.manifest.instructions()
    }

    #[must_use]
    pub fn messages(&self) -> &[CanonicalMessage] {
        &self.messages
    }

    #[must_use]
    pub fn tools(&self) -> &[CanonicalToolDefinition] {
        self.manifest.tools()
    }

    #[must_use]
    pub const fn journal_revision(&self) -> u64 {
        self.journal_revision
    }
}

fn require_text(value: &str, field: &'static str) -> Result<(), ContextError> {
    if value.trim().is_empty() {
        Err(ContextError::EmptyField(field))
    } else {
        Ok(())
    }
}

fn require_digest(value: &str) -> Result<(), ContextError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ContextError::InvalidDigest);
    };
    if hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ContextError::InvalidDigest)
    }
}
