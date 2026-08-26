//! Source-linked, Provider-independent retrieval vocabulary for long Agent Runs.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::agent::AgentRunId;
use crate::constants::{
    CONTEXT_ESTIMATED_BYTES_PER_TOKEN, CONTEXT_RETRIEVAL_FORMAT_REVISION,
    CONTEXT_RETRIEVAL_MAX_EXCERPT_CHARS, CONTEXT_RETRIEVAL_MAX_HITS,
    CONTEXT_RETRIEVAL_MAX_SEARCH_CHARS, CONTEXT_RETRIEVAL_MAX_TOKENS,
};
use crate::context::{ContextError, InferenceItemId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRetrievalReason {
    CurrentInputSimilarity,
    ExactSourceReference,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRetrievalSourceType {
    CreatorMessage,
    AssistantMessage,
    ToolRequest,
    ToolResult,
}

impl ContextRetrievalSourceType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreatorMessage => "creator_message",
            Self::AssistantMessage => "assistant_message",
            Self::ToolRequest => "tool_request",
            Self::ToolResult => "tool_result",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextRetrievalQuery {
    run_id: AgentRunId,
    search_text: Option<String>,
    exact_item_ids: Vec<InferenceItemId>,
    excluded_item_ids: Vec<InferenceItemId>,
    source_types: Vec<ContextRetrievalSourceType>,
    reason: ContextRetrievalReason,
    max_hits: u16,
    max_tokens: u64,
    fingerprint: String,
}

impl ContextRetrievalQuery {
    /// Creates one bounded Run-local query. Search text is not persisted in a Manifest;
    /// its deterministic fingerprint and the selected source-linked snippets are.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for an empty, unbounded, duplicate, or contradictory query.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: AgentRunId,
        search_text: Option<String>,
        exact_item_ids: Vec<InferenceItemId>,
        excluded_item_ids: Vec<InferenceItemId>,
        reason: ContextRetrievalReason,
        max_hits: u16,
        max_tokens: u64,
    ) -> Result<Self, ContextError> {
        let search_text = search_text
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if search_text.is_none() && exact_item_ids.is_empty() {
            return Err(ContextError::InvalidRetrievalQuery(
                "search text or an exact item id is required",
            ));
        }
        if search_text
            .as_ref()
            .is_some_and(|value| value.chars().count() > CONTEXT_RETRIEVAL_MAX_SEARCH_CHARS)
        {
            return Err(ContextError::InvalidRetrievalQuery(
                "search text exceeds the bounded size",
            ));
        }
        if max_hits == 0 || max_hits > CONTEXT_RETRIEVAL_MAX_HITS {
            return Err(ContextError::InvalidRetrievalQuery(
                "max hits is outside the supported range",
            ));
        }
        if max_tokens == 0 || max_tokens > CONTEXT_RETRIEVAL_MAX_TOKENS {
            return Err(ContextError::InvalidRetrievalQuery(
                "token budget is outside the supported range",
            ));
        }
        let exact = exact_item_ids.iter().collect::<HashSet<_>>();
        let excluded = excluded_item_ids.iter().collect::<HashSet<_>>();
        if exact.len() != exact_item_ids.len()
            || excluded.len() != excluded_item_ids.len()
            || exact.iter().any(|item_id| excluded.contains(item_id))
        {
            return Err(ContextError::InvalidRetrievalQuery(
                "item ids must be unique and exact ids cannot be excluded",
            ));
        }
        let fingerprint = query_fingerprint(
            &run_id,
            search_text.as_deref(),
            &exact_item_ids,
            &excluded_item_ids,
            &[],
            reason,
            max_hits,
            max_tokens,
        )?;
        Ok(Self {
            run_id,
            search_text,
            exact_item_ids,
            excluded_item_ids,
            source_types: Vec::new(),
            reason,
            max_hits,
            max_tokens,
            fingerprint,
        })
    }

    /// Restricts this query to selected durable Transcript source types.
    /// An empty list means all retrievable types.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when a source type is duplicated.
    pub fn with_source_types(
        mut self,
        source_types: Vec<ContextRetrievalSourceType>,
    ) -> Result<Self, ContextError> {
        if source_types.iter().collect::<HashSet<_>>().len() != source_types.len() {
            return Err(ContextError::InvalidRetrievalQuery(
                "source type filters must be unique",
            ));
        }
        self.source_types = source_types;
        self.fingerprint = query_fingerprint(
            &self.run_id,
            self.search_text.as_deref(),
            &self.exact_item_ids,
            &self.excluded_item_ids,
            &self.source_types,
            self.reason,
            self.max_hits,
            self.max_tokens,
        )?;
        Ok(self)
    }

    #[must_use]
    pub const fn run_id(&self) -> &AgentRunId {
        &self.run_id
    }

    #[must_use]
    pub fn search_text(&self) -> Option<&str> {
        self.search_text.as_deref()
    }

    #[must_use]
    pub fn exact_item_ids(&self) -> &[InferenceItemId] {
        &self.exact_item_ids
    }

    #[must_use]
    pub fn excluded_item_ids(&self) -> &[InferenceItemId] {
        &self.excluded_item_ids
    }

    #[must_use]
    pub fn source_types(&self) -> &[ContextRetrievalSourceType] {
        &self.source_types
    }

    #[must_use]
    pub const fn reason(&self) -> ContextRetrievalReason {
        self.reason
    }

    #[must_use]
    pub const fn max_hits(&self) -> u16 {
        self.max_hits
    }

    #[must_use]
    pub const fn max_tokens(&self) -> u64 {
        self.max_tokens
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRetrievalHit {
    item_id: InferenceItemId,
    source_type: ContextRetrievalSourceType,
    created_at_unix_millis: u64,
    project_revision: u64,
    content_hash: String,
    excerpt: String,
    execution_id: Option<String>,
    is_error: bool,
    rank_micros: i64,
    estimated_tokens: u64,
}

impl ContextRetrievalHit {
    /// Creates one bounded, source-linked retrieval hit from the durable Transcript.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when provenance, content, or bounds are invalid.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_id: InferenceItemId,
        source_type: ContextRetrievalSourceType,
        created_at_unix_millis: u64,
        project_revision: u64,
        content_hash: String,
        excerpt: String,
        execution_id: Option<String>,
        is_error: bool,
        rank_micros: i64,
    ) -> Result<Self, ContextError> {
        let mut hit = Self {
            item_id,
            source_type,
            created_at_unix_millis,
            project_revision,
            content_hash,
            excerpt,
            execution_id,
            is_error,
            rank_micros,
            estimated_tokens: 0,
        };
        hit.estimated_tokens = estimate_tokens(hit.model_visible_content().len())?;
        hit.validate()?;
        Ok(hit)
    }

    #[must_use]
    pub const fn item_id(&self) -> &InferenceItemId {
        &self.item_id
    }

    #[must_use]
    pub const fn source_type(&self) -> ContextRetrievalSourceType {
        self.source_type
    }

    #[must_use]
    pub const fn created_at_unix_millis(&self) -> u64 {
        self.created_at_unix_millis
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    #[must_use]
    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    #[must_use]
    pub fn execution_id(&self) -> Option<&str> {
        self.execution_id.as_deref()
    }

    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.is_error
    }

    #[must_use]
    pub const fn rank_micros(&self) -> i64 {
        self.rank_micros
    }

    #[must_use]
    pub const fn estimated_tokens(&self) -> u64 {
        self.estimated_tokens
    }

    #[must_use]
    pub fn model_visible_content(&self) -> String {
        format!(
            "[UNTRUSTED RETRIEVED CONTEXT]\nsourceItem:{}\nsourceType:{}\ncreatedAtUnixMillis:{}\nprojectRevision:{}\ncontentHash:{}\nexecutionId:{}\nisError:{}\ncontent:{}",
            self.item_id.as_str(),
            self.source_type.as_str(),
            self.created_at_unix_millis,
            self.project_revision,
            self.content_hash,
            self.execution_id.as_deref().unwrap_or("none"),
            self.is_error,
            self.excerpt
        )
    }

    /// Revalidates one hit restored from a Manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when its provenance or derived token cost is invalid.
    pub fn validate(&self) -> Result<(), ContextError> {
        if self.excerpt.trim().is_empty()
            || self.excerpt.chars().count() > CONTEXT_RETRIEVAL_MAX_EXCERPT_CHARS
        {
            return Err(ContextError::InvalidRetrievalResult(
                "excerpt is empty or unbounded",
            ));
        }
        if !is_digest(&self.content_hash) {
            return Err(ContextError::InvalidRetrievalResult(
                "source content hash is invalid",
            ));
        }
        if self
            .execution_id
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ContextError::InvalidRetrievalResult(
                "execution id must not be empty",
            ));
        }
        if self.source_type != ContextRetrievalSourceType::ToolResult
            && (self.execution_id.is_some() || self.is_error)
        {
            return Err(ContextError::InvalidRetrievalResult(
                "only Tool Results may carry execution or error provenance",
            ));
        }
        if self.estimated_tokens != estimate_tokens(self.model_visible_content().len())? {
            return Err(ContextError::InvalidRetrievalResult(
                "estimated token cost does not match the injected content",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRetrievalSelection {
    query_fingerprint: String,
    reason: ContextRetrievalReason,
    hits: Vec<ContextRetrievalHit>,
    estimated_tokens: u64,
    format_revision: String,
}

impl ContextRetrievalSelection {
    /// Freezes the exact retrieval material selected for one Context Manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for an empty, duplicate, or over-budget selection.
    pub fn new(
        query: &ContextRetrievalQuery,
        hits: Vec<ContextRetrievalHit>,
    ) -> Result<Self, ContextError> {
        if hits.is_empty() {
            return Err(ContextError::InvalidRetrievalResult(
                "selection must contain at least one hit",
            ));
        }
        let estimated_tokens =
            hits.iter().try_fold(0_u64, |total, hit| {
                total.checked_add(hit.estimated_tokens()).ok_or(
                    ContextError::InvalidRetrievalResult("selection token cost overflowed"),
                )
            })?;
        if hits.len() > usize::from(query.max_hits()) || estimated_tokens > query.max_tokens() {
            return Err(ContextError::InvalidRetrievalResult(
                "selection exceeds its query budget",
            ));
        }
        let selection = Self {
            query_fingerprint: query.fingerprint().to_owned(),
            reason: query.reason(),
            hits,
            estimated_tokens,
            format_revision: CONTEXT_RETRIEVAL_FORMAT_REVISION.to_owned(),
        };
        selection.validate()?;
        Ok(selection)
    }

    #[must_use]
    pub fn query_fingerprint(&self) -> &str {
        &self.query_fingerprint
    }

    #[must_use]
    pub const fn reason(&self) -> ContextRetrievalReason {
        self.reason
    }

    #[must_use]
    pub fn hits(&self) -> &[ContextRetrievalHit] {
        &self.hits
    }

    #[must_use]
    pub const fn estimated_tokens(&self) -> u64 {
        self.estimated_tokens
    }

    /// Revalidates retrieval audit data restored from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for unsupported, duplicate, corrupt, or inconsistent data.
    pub fn validate(&self) -> Result<(), ContextError> {
        if self.format_revision != CONTEXT_RETRIEVAL_FORMAT_REVISION
            || !is_digest(&self.query_fingerprint)
            || self.hits.is_empty()
            || self.hits.len() > usize::from(CONTEXT_RETRIEVAL_MAX_HITS)
            || self.estimated_tokens == 0
            || self.estimated_tokens > CONTEXT_RETRIEVAL_MAX_TOKENS
        {
            return Err(ContextError::InvalidRetrievalResult(
                "selection metadata is invalid",
            ));
        }
        let mut ids = HashSet::new();
        let mut tokens = 0_u64;
        for hit in &self.hits {
            hit.validate()?;
            if !ids.insert(hit.item_id()) {
                return Err(ContextError::InvalidRetrievalResult(
                    "selection contains a duplicate source item",
                ));
            }
            tokens = tokens.checked_add(hit.estimated_tokens()).ok_or(
                ContextError::InvalidRetrievalResult("selection token cost overflowed"),
            )?;
        }
        if tokens != self.estimated_tokens {
            return Err(ContextError::InvalidRetrievalResult(
                "selection token cost is inconsistent",
            ));
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn query_fingerprint(
    run_id: &AgentRunId,
    search_text: Option<&str>,
    exact_item_ids: &[InferenceItemId],
    excluded_item_ids: &[InferenceItemId],
    source_types: &[ContextRetrievalSourceType],
    reason: ContextRetrievalReason,
    max_hits: u16,
    max_tokens: u64,
) -> Result<String, ContextError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Fingerprint<'a> {
        run_id: &'a AgentRunId,
        search_text: Option<&'a str>,
        exact_item_ids: &'a [InferenceItemId],
        excluded_item_ids: &'a [InferenceItemId],
        source_types: &'a [ContextRetrievalSourceType],
        reason: ContextRetrievalReason,
        max_hits: u16,
        max_tokens: u64,
        format_revision: &'static str,
    }
    let bytes = serde_json::to_vec(&Fingerprint {
        run_id,
        search_text,
        exact_item_ids,
        excluded_item_ids,
        source_types,
        reason,
        max_hits,
        max_tokens,
        format_revision: CONTEXT_RETRIEVAL_FORMAT_REVISION,
    })
    .map_err(|error| ContextError::Serialization(error.to_string()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn estimate_tokens(bytes: usize) -> Result<u64, ContextError> {
    let bytes = u64::try_from(bytes)
        .map_err(|_| ContextError::InvalidRetrievalResult("content size exceeds u64"))?;
    bytes
        .checked_add(CONTEXT_ESTIMATED_BYTES_PER_TOKEN - 1)
        .map(|value| value / CONTEXT_ESTIMATED_BYTES_PER_TOKEN)
        .ok_or(ContextError::InvalidRetrievalResult(
            "token estimate overflowed",
        ))
}

fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
