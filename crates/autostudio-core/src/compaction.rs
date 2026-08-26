//! Provider-independent compaction checkpoint vocabulary.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::agent::AgentRunId;
use crate::constants::{
    COMPACTION_FORMAT_REVISION, MAX_COMPACTION_SUMMARY_FIELD_CHARS, MAX_COMPACTION_SUMMARY_ITEMS,
};
use crate::context::InferenceItemId;
pub use crate::error::CompactionError;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CompactionId(Uuid);

impl CompactionId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parses a durable compaction identity.
    ///
    /// # Errors
    ///
    /// Returns [`CompactionError::InvalidId`] for malformed input.
    pub fn parse(value: &str) -> Result<Self, CompactionError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| CompactionError::InvalidId)
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for CompactionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredRunSummaryDraft {
    pub objective: String,
    pub creator_decisions: Vec<String>,
    pub constraints: Vec<String>,
    pub completed_work: Vec<String>,
    pub open_items: Vec<String>,
    pub artifact_references: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredRunSummary {
    objective: String,
    creator_decisions: Vec<String>,
    constraints: Vec<String>,
    completed_work: Vec<String>,
    open_items: Vec<String>,
    artifact_references: Vec<String>,
}

impl StructuredRunSummary {
    /// Creates the host-owned summary that can replace older model-visible items.
    ///
    /// # Errors
    ///
    /// Returns [`CompactionError`] when the objective or any list item is empty,
    /// or when the bounded summary limits are exceeded.
    pub fn new(draft: StructuredRunSummaryDraft) -> Result<Self, CompactionError> {
        let summary = Self {
            objective: draft.objective,
            creator_decisions: draft.creator_decisions,
            constraints: draft.constraints,
            completed_work: draft.completed_work,
            open_items: draft.open_items,
            artifact_references: draft.artifact_references,
        };
        summary.validate()?;
        Ok(summary)
    }

    #[must_use]
    pub fn objective(&self) -> &str {
        &self.objective
    }

    #[must_use]
    pub fn creator_decisions(&self) -> &[String] {
        &self.creator_decisions
    }

    #[must_use]
    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    #[must_use]
    pub fn completed_work(&self) -> &[String] {
        &self.completed_work
    }

    #[must_use]
    pub fn open_items(&self) -> &[String] {
        &self.open_items
    }

    #[must_use]
    pub fn artifact_references(&self) -> &[String] {
        &self.artifact_references
    }

    /// Revalidates a summary restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`CompactionError`] for empty or unbounded content.
    pub fn validate(&self) -> Result<(), CompactionError> {
        validate_summary_text(&self.objective, "objective")?;
        let lists = [
            self.creator_decisions.as_slice(),
            self.constraints.as_slice(),
            self.completed_work.as_slice(),
            self.open_items.as_slice(),
            self.artifact_references.as_slice(),
        ];
        let item_count = lists.iter().map(|items| items.len()).sum::<usize>();
        if item_count > MAX_COMPACTION_SUMMARY_ITEMS {
            return Err(CompactionError::TooManySummaryItems);
        }
        for item in lists.into_iter().flatten() {
            validate_summary_text(item, "summary item")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionCheckpoint {
    compaction_id: CompactionId,
    run_id: AgentRunId,
    source_journal_revision: u64,
    replaces_item_ids: Vec<InferenceItemId>,
    first_kept_item_id: InferenceItemId,
    summary: StructuredRunSummary,
    created_at_unix_millis: u64,
    format_revision: String,
    content_hash: String,
}

impl CompactionCheckpoint {
    /// Creates an immutable checkpoint. The content hash intentionally excludes
    /// the random checkpoint id and observation time, so the same source facts
    /// produce the same hash.
    ///
    /// # Errors
    ///
    /// Returns [`CompactionError`] for an empty/duplicate replacement set, an
    /// invalid cut point, an empty source revision, or invalid summary content.
    pub fn new(
        run_id: AgentRunId,
        source_journal_revision: u64,
        replaces_item_ids: Vec<InferenceItemId>,
        first_kept_item_id: InferenceItemId,
        summary: StructuredRunSummary,
        created_at_unix_millis: u64,
    ) -> Result<Self, CompactionError> {
        let mut checkpoint = Self {
            compaction_id: CompactionId::new(),
            run_id,
            source_journal_revision,
            replaces_item_ids,
            first_kept_item_id,
            summary,
            created_at_unix_millis,
            format_revision: COMPACTION_FORMAT_REVISION.to_owned(),
            content_hash: String::new(),
        };
        checkpoint.validate_shape()?;
        checkpoint.content_hash = checkpoint.calculate_content_hash()?;
        Ok(checkpoint)
    }

    #[must_use]
    pub const fn id(&self) -> &CompactionId {
        &self.compaction_id
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    #[must_use]
    pub const fn source_journal_revision(&self) -> u64 {
        self.source_journal_revision
    }

    #[must_use]
    pub fn replaces_item_ids(&self) -> &[InferenceItemId] {
        &self.replaces_item_ids
    }

    #[must_use]
    pub const fn first_kept_item_id(&self) -> &InferenceItemId {
        &self.first_kept_item_id
    }

    #[must_use]
    pub const fn summary(&self) -> &StructuredRunSummary {
        &self.summary
    }

    #[must_use]
    pub const fn created_at_unix_millis(&self) -> u64 {
        self.created_at_unix_millis
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// Revalidates a checkpoint restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`CompactionError`] when its shape, format, or hash is invalid.
    pub fn validate(&self) -> Result<(), CompactionError> {
        self.validate_shape()?;
        if self.content_hash != self.calculate_content_hash()? {
            return Err(CompactionError::ContentHashMismatch);
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), CompactionError> {
        if self.source_journal_revision == 0 {
            return Err(CompactionError::InvalidSourceRevision);
        }
        if self.replaces_item_ids.is_empty() {
            return Err(CompactionError::EmptyReplacementSet);
        }
        let unique = self
            .replaces_item_ids
            .iter()
            .collect::<HashSet<&InferenceItemId>>();
        if unique.len() != self.replaces_item_ids.len() || unique.contains(&self.first_kept_item_id)
        {
            return Err(CompactionError::InvalidReplacementSet);
        }
        if self.format_revision != COMPACTION_FORMAT_REVISION {
            return Err(CompactionError::UnsupportedFormat);
        }
        self.summary.validate()
    }

    fn calculate_content_hash(&self) -> Result<String, CompactionError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct HashInput<'a> {
            run_id: &'a AgentRunId,
            source_journal_revision: u64,
            replaces_item_ids: &'a [InferenceItemId],
            first_kept_item_id: &'a InferenceItemId,
            summary: &'a StructuredRunSummary,
            format_revision: &'a str,
        }

        let bytes = serde_json::to_vec(&HashInput {
            run_id: &self.run_id,
            source_journal_revision: self.source_journal_revision,
            replaces_item_ids: &self.replaces_item_ids,
            first_kept_item_id: &self.first_kept_item_id,
            summary: &self.summary,
            format_revision: &self.format_revision,
        })
        .map_err(|error| CompactionError::Serialization(error.to_string()))?;
        Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
    }
}

fn validate_summary_text(value: &str, field: &'static str) -> Result<(), CompactionError> {
    if value.trim().is_empty() {
        return Err(CompactionError::EmptySummaryField(field));
    }
    if value.chars().count() > MAX_COMPACTION_SUMMARY_FIELD_CHARS {
        return Err(CompactionError::SummaryFieldTooLong(field));
    }
    Ok(())
}
