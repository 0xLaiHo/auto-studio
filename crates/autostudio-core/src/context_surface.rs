//! Deterministic, Provider-independent Context Surface measurement and spill vocabulary.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::constants::{
    CONTEXT_ESTIMATED_BYTES_PER_TOKEN, CONTEXT_HARD_PRESSURE_PERCENT,
    CONTEXT_SOFT_PRESSURE_PERCENT, CONTEXT_SURFACE_FORMAT_REVISION,
    CONTEXT_SURFACE_LEGACY_FORMAT_REVISION,
};
use crate::context::{CanonicalMessage, CanonicalToolDefinition, InferenceItemId, TokenBudgetPlan};
pub use crate::error::ContextSurfaceError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPressure {
    Unknown,
    Normal,
    Soft,
    Hard,
    Overflow,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSurfaceTransform {
    #[default]
    None,
    ToolResultSpill,
    Compaction,
    CompactionAndToolResultSpill,
}

impl ContextSurfaceTransform {
    #[must_use]
    pub const fn includes_compaction(self) -> bool {
        matches!(self, Self::Compaction | Self::CompactionAndToolResultSpill)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPreparationReason {
    #[default]
    Standard,
    PressureCompaction,
    ProviderOverflowRecovery,
}

impl ContextPreparationReason {
    #[must_use]
    pub const fn requires_compaction(self) -> bool {
        !matches!(self, Self::Standard)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFootprint {
    instructions_bytes: u64,
    tool_schema_bytes: u64,
    message_bytes: u64,
    continuity_overhead_tokens: u64,
    total_serialized_bytes: u64,
    estimated_input_tokens: u64,
    input_budget_tokens: Option<u64>,
    pressure: ContextPressure,
}

impl ContextFootprint {
    /// Measures canonical request bytes plus an Adapter-supplied opaque
    /// continuity allowance.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError`] when serialization or numeric conversion fails.
    pub fn measure(
        instructions: &str,
        messages: &[CanonicalMessage],
        tools: &[CanonicalToolDefinition],
        continuity_overhead_tokens: u64,
        token_budget: &TokenBudgetPlan,
    ) -> Result<Self, ContextSurfaceError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct CanonicalInput<'a> {
            instructions: &'a str,
            messages: &'a [CanonicalMessage],
            tools: &'a [CanonicalToolDefinition],
        }

        let instructions_bytes = serialized_len(instructions)?;
        let tool_schema_bytes = serialized_len(tools)?;
        let message_bytes = serialized_len(messages)?;
        let total_serialized_bytes = serialized_len(&CanonicalInput {
            instructions,
            messages,
            tools,
        })?;
        let serialized_tokens = estimate_tokens(total_serialized_bytes)?;
        let estimated_input_tokens = serialized_tokens
            .checked_add(continuity_overhead_tokens)
            .ok_or(ContextSurfaceError::FootprintOverflow)?;
        let input_budget_tokens = token_budget.input_budget_tokens;
        let pressure = pressure_for(estimated_input_tokens, input_budget_tokens);
        Ok(Self {
            instructions_bytes,
            tool_schema_bytes,
            message_bytes,
            continuity_overhead_tokens,
            total_serialized_bytes,
            estimated_input_tokens,
            input_budget_tokens,
            pressure,
        })
    }

    #[must_use]
    pub const fn instructions_bytes(&self) -> u64 {
        self.instructions_bytes
    }

    #[must_use]
    pub const fn tool_schema_bytes(&self) -> u64 {
        self.tool_schema_bytes
    }

    #[must_use]
    pub const fn message_bytes(&self) -> u64 {
        self.message_bytes
    }

    #[must_use]
    pub const fn continuity_overhead_tokens(&self) -> u64 {
        self.continuity_overhead_tokens
    }

    #[must_use]
    pub const fn total_serialized_bytes(&self) -> u64 {
        self.total_serialized_bytes
    }

    #[must_use]
    pub const fn estimated_input_tokens(&self) -> u64 {
        self.estimated_input_tokens
    }

    #[must_use]
    pub const fn input_budget_tokens(&self) -> Option<u64> {
        self.input_budget_tokens
    }

    #[must_use]
    pub const fn pressure(&self) -> ContextPressure {
        self.pressure
    }

    /// Revalidates deterministic values restored from a Context Manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError::InvalidFootprint`] when stored derived
    /// values disagree with the versioned measurement policy.
    pub fn validate(&self) -> Result<(), ContextSurfaceError> {
        let expected_tokens = estimate_tokens(self.total_serialized_bytes)?
            .checked_add(self.continuity_overhead_tokens)
            .ok_or(ContextSurfaceError::FootprintOverflow)?;
        if self.total_serialized_bytes == 0
            || self.instructions_bytes > self.total_serialized_bytes
            || self.tool_schema_bytes > self.total_serialized_bytes
            || self.message_bytes > self.total_serialized_bytes
            || self.estimated_input_tokens != expected_tokens
            || self.pressure != pressure_for(expected_tokens, self.input_budget_tokens)
        {
            return Err(ContextSurfaceError::InvalidFootprint);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSpillBlob {
    content_hash: String,
    content: String,
}

impl ContextSpillBlob {
    /// Creates one immutable content-addressed Tool Result spill.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError::EmptySpillContent`] for empty content.
    pub fn new(content: String) -> Result<Self, ContextSurfaceError> {
        if content.is_empty() {
            return Err(ContextSurfaceError::EmptySpillContent);
        }
        let content_hash = digest(content.as_bytes());
        Ok(Self {
            content_hash,
            content,
        })
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub fn byte_count(&self) -> usize {
        self.content.len()
    }

    /// Revalidates bytes restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError`] for empty or hash-mismatched content.
    pub fn validate(&self) -> Result<(), ContextSurfaceError> {
        if self.content.is_empty() {
            return Err(ContextSurfaceError::EmptySpillContent);
        }
        if self.content_hash != digest(self.content.as_bytes()) {
            return Err(ContextSurfaceError::SpillHashMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultSpillReference {
    item_id: InferenceItemId,
    content_hash: String,
    original_bytes: u64,
    retained_preview: String,
}

impl ToolResultSpillReference {
    /// Creates the model-visible locator for a spilled Tool Result.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError`] for empty previews, invalid digests, or
    /// impossible byte counts.
    pub fn new(
        item_id: InferenceItemId,
        blob: &ContextSpillBlob,
        retained_preview: String,
    ) -> Result<Self, ContextSurfaceError> {
        let original_bytes =
            u64::try_from(blob.byte_count()).map_err(|_| ContextSurfaceError::FootprintOverflow)?;
        let reference = Self {
            item_id,
            content_hash: blob.content_hash().to_owned(),
            original_bytes,
            retained_preview,
        };
        reference.validate()?;
        Ok(reference)
    }

    #[must_use]
    pub const fn item_id(&self) -> &InferenceItemId {
        &self.item_id
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    #[must_use]
    pub const fn original_bytes(&self) -> u64 {
        self.original_bytes
    }

    #[must_use]
    pub fn retained_preview(&self) -> &str {
        &self.retained_preview
    }

    /// Revalidates a reference restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError::InvalidSpillReference`] for invalid fields.
    pub fn validate(&self) -> Result<(), ContextSurfaceError> {
        if self.original_bytes == 0
            || self.retained_preview.is_empty()
            || !is_digest(&self.content_hash)
        {
            return Err(ContextSurfaceError::InvalidSpillReference);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSurfaceMetrics {
    initial_footprint: ContextFootprint,
    prepared_footprint: ContextFootprint,
    spills: Vec<ToolResultSpillReference>,
    #[serde(default)]
    transform: ContextSurfaceTransform,
    format_revision: String,
}

impl ContextSurfaceMetrics {
    #[must_use]
    pub fn new(
        initial_footprint: ContextFootprint,
        prepared_footprint: ContextFootprint,
        spills: Vec<ToolResultSpillReference>,
        compaction_applied: bool,
    ) -> Self {
        let transform = match (compaction_applied, spills.is_empty()) {
            (false, true) => ContextSurfaceTransform::None,
            (false, false) => ContextSurfaceTransform::ToolResultSpill,
            (true, true) => ContextSurfaceTransform::Compaction,
            (true, false) => ContextSurfaceTransform::CompactionAndToolResultSpill,
        };
        Self {
            initial_footprint,
            prepared_footprint,
            spills,
            transform,
            format_revision: CONTEXT_SURFACE_FORMAT_REVISION.to_owned(),
        }
    }

    #[must_use]
    pub const fn initial_footprint(&self) -> &ContextFootprint {
        &self.initial_footprint
    }

    #[must_use]
    pub const fn prepared_footprint(&self) -> &ContextFootprint {
        &self.prepared_footprint
    }

    #[must_use]
    pub fn spills(&self) -> &[ToolResultSpillReference] {
        &self.spills
    }

    #[must_use]
    pub const fn transform(&self) -> ContextSurfaceTransform {
        self.transform
    }

    /// Revalidates surface audit metadata restored from a Manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ContextSurfaceError`] for unsupported or malformed spill data.
    pub fn validate(&self) -> Result<(), ContextSurfaceError> {
        if self.format_revision != CONTEXT_SURFACE_FORMAT_REVISION
            && self.format_revision != CONTEXT_SURFACE_LEGACY_FORMAT_REVISION
        {
            return Err(ContextSurfaceError::InvalidSpillReference);
        }
        self.initial_footprint.validate()?;
        self.prepared_footprint.validate()?;
        if self.initial_footprint.instructions_bytes()
            != self.prepared_footprint.instructions_bytes()
            || self.initial_footprint.tool_schema_bytes()
                != self.prepared_footprint.tool_schema_bytes()
            || self.initial_footprint.continuity_overhead_tokens()
                != self.prepared_footprint.continuity_overhead_tokens()
            || self.initial_footprint.input_budget_tokens()
                != self.prepared_footprint.input_budget_tokens()
        {
            return Err(ContextSurfaceError::InvalidFootprint);
        }
        let mut item_ids = HashSet::new();
        for spill in &self.spills {
            spill.validate()?;
            if !item_ids.insert(spill.item_id()) {
                return Err(ContextSurfaceError::InvalidSpillReference);
            }
        }
        let effective_transform = if self.format_revision == CONTEXT_SURFACE_LEGACY_FORMAT_REVISION
        {
            if self.spills.is_empty() {
                ContextSurfaceTransform::None
            } else {
                ContextSurfaceTransform::ToolResultSpill
            }
        } else {
            self.transform
        };
        let transform_matches_spills = matches!(
            (effective_transform, self.spills.is_empty()),
            (
                ContextSurfaceTransform::None | ContextSurfaceTransform::Compaction,
                true
            ) | (
                ContextSurfaceTransform::ToolResultSpill
                    | ContextSurfaceTransform::CompactionAndToolResultSpill,
                false
            )
        );
        if !transform_matches_spills {
            return Err(ContextSurfaceError::InvalidSpillReference);
        }
        if effective_transform == ContextSurfaceTransform::None {
            if self.initial_footprint != self.prepared_footprint {
                return Err(ContextSurfaceError::InvalidSpillReference);
            }
        } else if self.prepared_footprint.message_bytes() >= self.initial_footprint.message_bytes()
            || self.prepared_footprint.total_serialized_bytes()
                >= self.initial_footprint.total_serialized_bytes()
        {
            return Err(ContextSurfaceError::InvalidSpillReference);
        }
        Ok(())
    }
}

fn serialized_len<T: Serialize + ?Sized>(value: &T) -> Result<u64, ContextSurfaceError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| ContextSurfaceError::Serialization(error.to_string()))?;
    u64::try_from(bytes.len()).map_err(|_| ContextSurfaceError::FootprintOverflow)
}

fn estimate_tokens(bytes: u64) -> Result<u64, ContextSurfaceError> {
    bytes
        .checked_add(CONTEXT_ESTIMATED_BYTES_PER_TOKEN - 1)
        .map(|value| value / CONTEXT_ESTIMATED_BYTES_PER_TOKEN)
        .ok_or(ContextSurfaceError::FootprintOverflow)
}

fn pressure_for(estimated_tokens: u64, input_budget_tokens: Option<u64>) -> ContextPressure {
    let Some(budget) = input_budget_tokens else {
        return ContextPressure::Unknown;
    };
    if estimated_tokens > budget {
        return ContextPressure::Overflow;
    }
    let utilization = u128::from(estimated_tokens) * 100;
    let budget = u128::from(budget);
    if utilization >= budget * u128::from(CONTEXT_HARD_PRESSURE_PERCENT) {
        ContextPressure::Hard
    } else if utilization >= budget * u128::from(CONTEXT_SOFT_PRESSURE_PERCENT) {
        ContextPressure::Soft
    } else {
        ContextPressure::Normal
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
