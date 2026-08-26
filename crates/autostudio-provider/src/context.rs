//! Run-scoped context assembly and durable transcript recording.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use autostudio_core::agent::AgentRunId;
use autostudio_core::compaction::{
    CompactionCheckpoint, StructuredRunSummary, StructuredRunSummaryDraft,
};
use autostudio_core::constants::{
    CONTEXT_COMPACTION_MIN_RECENT_TURNS, CONTEXT_COMPACTION_SUMMARY_ITEM_CHARS,
    CONTEXT_COMPACTION_SUMMARY_MAX_ITEMS, CONTEXT_COMPACTION_SUMMARY_OBJECTIVE_CHARS,
    CONTEXT_COMPACTION_UNKNOWN_TARGET_PERCENT, CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS,
    CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS, CONTEXT_TOOL_RESULT_PREVIEW_CHARS,
    CONTEXT_TOOL_RESULT_SPILL_THRESHOLD_BYTES,
};
use autostudio_core::context::{
    CanonicalMessage, CanonicalToolCall, CanonicalToolDefinition, ContextError, ContextEvent,
    ContextEventStore, ContextId, ContextManifest, InferenceItem, InferenceItemDraft,
    InferenceTurnId, PreparedContext, ProviderBinding, TokenBudgetPlan, VisibleMessageRole,
};
use autostudio_core::context_retrieval::{
    ContextRetrievalQuery, ContextRetrievalReason, ContextRetrievalSelection,
};
use autostudio_core::context_surface::{
    ContextFootprint, ContextPreparationReason, ContextPressure, ContextSpillBlob,
    ContextSurfaceMetrics, ToolResultSpillReference,
};
use autostudio_core::continuity::ContinuityReference;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Complete input required to prepare one model-visible Inference Turn.
pub struct PrepareContext {
    pub run_id: AgentRunId,
    pub turn_id: InferenceTurnId,
    pub project_id: String,
    pub project_revision: u64,
    pub instructions: String,
    pub new_user_messages: Vec<String>,
    pub provider_binding: ProviderBinding,
    pub continuity_reference: Option<ContinuityReference>,
    pub continuity_overhead_tokens: u64,
    pub tools: Vec<CanonicalToolDefinition>,
    pub token_budget: TokenBudgetPlan,
}

/// Complete Provider output to append after one Inference Turn ends.
pub struct RecordInferenceTurn {
    pub run_id: AgentRunId,
    pub turn_id: InferenceTurnId,
    pub context_id: ContextId,
    pub expected_journal_revision: u64,
    pub items: Vec<InferenceItemDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedInferenceTurn {
    pub journal_revision: u64,
    pub items: Vec<InferenceItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingToolRequest {
    pub turn_id: InferenceTurnId,
    pub call_id: String,
    pub name: String,
    pub arguments_json: String,
    pub descriptor_fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextProjection {
    journal_revision: u64,
    items: Vec<InferenceItem>,
    manifests: Vec<ContextManifest>,
    checkpoints: Vec<CompactionCheckpoint>,
    pending_tools: Vec<PendingToolRequest>,
    prepared_turn_without_output: Option<InferenceTurnId>,
}

impl ContextProjection {
    #[must_use]
    pub const fn journal_revision(&self) -> u64 {
        self.journal_revision
    }

    #[must_use]
    pub fn items(&self) -> &[InferenceItem] {
        &self.items
    }

    #[must_use]
    pub fn manifests(&self) -> &[ContextManifest] {
        &self.manifests
    }

    #[must_use]
    pub fn checkpoints(&self) -> &[CompactionCheckpoint] {
        &self.checkpoints
    }

    #[must_use]
    pub fn pending_tools(&self) -> &[PendingToolRequest] {
        &self.pending_tools
    }

    #[must_use]
    pub const fn prepared_turn_without_output(&self) -> Option<&InferenceTurnId> {
        self.prepared_turn_without_output.as_ref()
    }
}

pub struct RecordToolResults {
    pub run_id: AgentRunId,
    pub expected_journal_revision: u64,
    pub results: Vec<CompletedToolResult>,
}

pub struct CompletedToolResult {
    pub call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
    pub execution_id: Option<String>,
}

/// Complete input for atomically publishing one compaction checkpoint.
pub struct CommitCompaction {
    pub run_id: AgentRunId,
    pub expected_journal_revision: u64,
    pub replaces_item_ids: Vec<autostudio_core::context::InferenceItemId>,
    pub first_kept_item_id: autostudio_core::context::InferenceItemId,
    pub summary: StructuredRunSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedCompaction {
    pub journal_revision: u64,
    pub checkpoint: CompactionCheckpoint,
}

/// Deep module that owns transcript replay, ordering, hashing, and manifests.
pub struct ContextManager {
    store: Arc<dyn ContextEventStore>,
}

impl ContextManager {
    #[must_use]
    pub fn new(store: Arc<dyn ContextEventStore>) -> Self {
        Self { store }
    }

    /// Replays one Run, appends the new Creator input, and persists the exact
    /// immutable Context Manifest before the Provider can be called.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for malformed input, corrupt replay, a stale
    /// journal revision, serialization failure, or unavailable storage.
    #[allow(clippy::too_many_lines)]
    pub fn prepare_turn(&self, request: PrepareContext) -> Result<PreparedContext, ContextError> {
        validate_prepare_request(&request)?;
        let state = replay(self.store.context_events(&request.run_id)?, &request.run_id)?;
        let projection = project_state(state.clone());
        if !projection.pending_tools.is_empty() {
            return Err(ContextError::InconsistentJournal(
                "pending Tool Requests must be completed before preparing the next turn".to_owned(),
            ));
        }
        if projection.prepared_turn_without_output.is_some() {
            return Err(ContextError::InconsistentJournal(
                "a prepared Inference Turn has no durable output".to_owned(),
            ));
        }
        if request.new_user_messages.is_empty() && projection.items.is_empty() {
            return Err(ContextError::EmptyField("prepare.new_user_messages"));
        }
        let created_at = now_unix_millis()?;
        let items = build_user_items(&request, state.next_item_sequence, created_at)?;
        let previous_checkpoint = state.checkpoints.last().cloned();
        let mut all_items = state.items.clone();
        all_items.extend(items.iter().cloned());
        let retrieval_selection = plan_context_retrieval(
            self.store.as_ref(),
            &request,
            &all_items,
            previous_checkpoint.as_ref(),
        )?;
        let SelectedPreparedSurface {
            new_checkpoint,
            surface_start,
            prepared_surface,
            preparation_reason,
            source_journal_revision,
        } = select_prepared_surface(
            &request,
            &state,
            &all_items,
            items.len(),
            retrieval_selection.as_ref(),
            created_at,
        )?;
        let checkpoint = new_checkpoint.as_ref().or(previous_checkpoint.as_ref());
        let surface_items = &all_items[surface_start..];
        let PreparedSurfaceArtifacts {
            messages,
            spills,
            metrics: surface_metrics,
        } = prepared_surface;
        let included_item_ids = surface_items.iter().map(|item| item.id().clone()).collect();
        let transcript_revision = source_journal_revision
            .checked_add(u64::from(new_checkpoint.is_some()))
            .ok_or(ContextError::SequenceExhausted)?;
        let content_hash = digest_json(&ContextHashInput {
            project_id: &request.project_id,
            project_revision: request.project_revision,
            instructions: &request.instructions,
            messages: &messages,
            tools: &request.tools,
            provider_binding: &request.provider_binding,
            compaction_checkpoint_hash: checkpoint.map(CompactionCheckpoint::content_hash),
            surface_metrics: &surface_metrics,
            preparation_reason,
            retrieval_selection: retrieval_selection.as_ref(),
            continuity_reference: request.continuity_reference.as_ref(),
            token_budget: &request.token_budget,
        })?;
        let manifest = ContextManifest::new(
            request.run_id.clone(),
            request.turn_id,
            request.project_id,
            request.project_revision,
            request.project_revision,
            transcript_revision,
            included_item_ids,
            request.instructions.clone(),
            request.tools.clone(),
            request.provider_binding,
            checkpoint.map(|checkpoint| checkpoint.id().clone()),
            Some(surface_metrics),
            preparation_reason,
            retrieval_selection,
            request.continuity_reference,
            request.token_budget,
            content_hash,
        )?;
        let mut events = items
            .iter()
            .cloned()
            .map(|item| ContextEvent::InferenceItemAppended { item })
            .collect::<Vec<_>>();
        if let Some(checkpoint) = new_checkpoint {
            events.push(ContextEvent::CompactionCommitted {
                checkpoint: Box::new(checkpoint),
            });
        }
        events.push(ContextEvent::ContextPrepared {
            manifest: Box::new(manifest.clone()),
        });
        let journal_revision = self.store.append_context_events_with_spills(
            &request.run_id,
            state.journal_revision,
            &events,
            &spills,
        )?;
        Ok(PreparedContext::new(manifest, messages, journal_revision))
    }

    /// Appends only complete canonical items after Provider streaming has been
    /// assembled. Private Provider reasoning is intentionally not an item type.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for an unknown Context, mismatched identity,
    /// corrupt replay, stale journal revision, or invalid output item.
    pub fn record_turn(
        &self,
        request: RecordInferenceTurn,
    ) -> Result<RecordedInferenceTurn, ContextError> {
        if request.items.is_empty() {
            return Err(ContextError::InconsistentJournal(
                "an Inference Turn must record at least one completed item".to_owned(),
            ));
        }
        validate_turn_items(&request.items)?;
        let state = replay(self.store.context_events(&request.run_id)?, &request.run_id)?;
        let manifest = validate_record_binding(&state, &request)?;

        let created_at = now_unix_millis()?;
        let mut next_item_sequence = state.next_item_sequence;
        let mut items = Vec::with_capacity(request.items.len());
        let mut call_ids = state
            .items
            .iter()
            .filter_map(|item| match item.payload() {
                InferenceItemDraft::ToolRequest { call_id, .. } => Some(call_id.clone()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        for payload in request.items {
            validate_provider_payload(&payload, manifest, &mut call_ids)?;
            let item = InferenceItem::new(
                request.run_id.clone(),
                request.turn_id.clone(),
                next_item_sequence,
                created_at,
                digest_json(&payload)?,
                payload,
            )?;
            next_item_sequence = next_item_sequence
                .checked_add(1)
                .ok_or(ContextError::SequenceExhausted)?;
            items.push(item);
        }
        let events = items
            .iter()
            .cloned()
            .map(|item| ContextEvent::InferenceItemAppended { item })
            .collect::<Vec<_>>();
        let journal_revision = self.store.append_context_events(
            &request.run_id,
            request.expected_journal_revision,
            &events,
        )?;
        Ok(RecordedInferenceTurn {
            journal_revision,
            items,
        })
    }

    /// Returns a validated durable projection used to decide the next Agent
    /// Step after process restart.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when storage or transcript invariants fail.
    pub fn inspect_run(&self, run_id: &AgentRunId) -> Result<ContextProjection, ContextError> {
        let state = replay(self.store.context_events(run_id)?, run_id)?;
        Ok(project_state(state))
    }

    /// Searches source-linked history through the same rebuildable Run-local
    /// retrieval projection used by automatic Context preparation.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] when retrieval storage is unavailable or corrupt.
    pub fn retrieve_context(
        &self,
        query: &ContextRetrievalQuery,
    ) -> Result<Option<ContextRetrievalSelection>, ContextError> {
        self.store
            .retrieve_context(query)
            .map_err(ContextError::from)
    }

    /// Validates and atomically appends a host-owned compaction checkpoint.
    /// The complete transcript remains untouched; only later model surfaces use
    /// the checkpoint summary plus the kept tail.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for stale revisions, non-prefix replacement,
    /// Tool pair splits, non-advancing checkpoints, or unavailable storage.
    pub fn commit_compaction(
        &self,
        request: CommitCompaction,
    ) -> Result<RecordedCompaction, ContextError> {
        let state = replay(self.store.context_events(&request.run_id)?, &request.run_id)?;
        if state.journal_revision != request.expected_journal_revision {
            return Err(
                autostudio_core::context::ContextStoreError::RevisionConflict {
                    expected: request.expected_journal_revision,
                    actual: state.journal_revision,
                }
                .into(),
            );
        }
        validate_compaction_cut(
            &state.items,
            state.checkpoints.last(),
            &request.replaces_item_ids,
            &request.first_kept_item_id,
        )?;
        let checkpoint = CompactionCheckpoint::new(
            request.run_id.clone(),
            state.journal_revision,
            request.replaces_item_ids,
            request.first_kept_item_id,
            request.summary,
            now_unix_millis()?,
        )
        .map_err(|error| ContextError::InconsistentJournal(error.to_string()))?;
        let journal_revision = self.store.append_context_events(
            &request.run_id,
            state.journal_revision,
            &[ContextEvent::CompactionCommitted {
                checkpoint: Box::new(checkpoint.clone()),
            }],
        )?;
        Ok(RecordedCompaction {
            journal_revision,
            checkpoint,
        })
    }

    /// Records complete local Tool Results only after matching every result to
    /// one pending durable Tool Request.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError`] for empty results, stale revision, orphaned or
    /// duplicate results, mismatched names, or storage failure.
    pub fn record_tool_results(
        &self,
        request: RecordToolResults,
    ) -> Result<RecordedInferenceTurn, ContextError> {
        if request.results.is_empty() {
            return Err(ContextError::InconsistentJournal(
                "at least one Tool Result is required".to_owned(),
            ));
        }
        let state = replay(self.store.context_events(&request.run_id)?, &request.run_id)?;
        if state.journal_revision != request.expected_journal_revision {
            return Err(
                autostudio_core::context::ContextStoreError::RevisionConflict {
                    expected: request.expected_journal_revision,
                    actual: state.journal_revision,
                }
                .into(),
            );
        }
        let projection = project_state(state);
        let created_at = now_unix_millis()?;
        let mut sequence = projection.items.last().map_or(Ok(1), |item| {
            item.sequence()
                .checked_add(1)
                .ok_or(ContextError::SequenceExhausted)
        })?;
        let mut items = Vec::with_capacity(request.results.len());
        let mut seen = std::collections::HashSet::new();
        for result in request.results {
            if !seen.insert(result.call_id.clone()) {
                return Err(ContextError::InconsistentJournal(
                    "one Tool Result batch contains a duplicate call id".to_owned(),
                ));
            }
            let pending = projection
                .pending_tools
                .iter()
                .find(|pending| pending.call_id == result.call_id)
                .ok_or_else(|| {
                    ContextError::InconsistentJournal(
                        "Tool Result does not match a pending Tool Request".to_owned(),
                    )
                })?;
            if pending.name != result.name {
                return Err(ContextError::InconsistentJournal(
                    "Tool Result name does not match its Tool Request".to_owned(),
                ));
            }
            let payload = InferenceItemDraft::ToolResult {
                call_id: result.call_id,
                name: result.name,
                content: result.content,
                is_error: result.is_error,
                execution_id: result.execution_id,
            };
            let item = InferenceItem::new(
                request.run_id.clone(),
                pending.turn_id.clone(),
                sequence,
                created_at,
                digest_json(&payload)?,
                payload,
            )?;
            sequence = sequence
                .checked_add(1)
                .ok_or(ContextError::SequenceExhausted)?;
            items.push(item);
        }
        let events = items
            .iter()
            .cloned()
            .map(|item| ContextEvent::InferenceItemAppended { item })
            .collect::<Vec<_>>();
        let journal_revision = self.store.append_context_events(
            &request.run_id,
            request.expected_journal_revision,
            &events,
        )?;
        Ok(RecordedInferenceTurn {
            journal_revision,
            items,
        })
    }
}

#[must_use]
pub fn fingerprint_tool_catalog(tools: &[CanonicalToolDefinition]) -> String {
    let mut catalog = Sha256::new();
    for tool in tools {
        catalog.update(Sha256::digest(tool.name.as_bytes()));
        catalog.update(Sha256::digest(tool.description.as_bytes()));
        catalog.update(Sha256::digest(tool.input_schema_json.as_bytes()));
        catalog.update(Sha256::digest(tool.descriptor_fingerprint.as_bytes()));
    }
    format!("sha256:{:x}", catalog.finalize())
}

#[derive(Clone)]
struct ReplayState {
    journal_revision: u64,
    next_item_sequence: u64,
    items: Vec<InferenceItem>,
    manifests: Vec<ContextManifest>,
    checkpoints: Vec<CompactionCheckpoint>,
}

fn project_state(state: ReplayState) -> ContextProjection {
    let mut requests = Vec::new();
    let mut completed = std::collections::HashSet::new();
    for item in &state.items {
        match item.payload() {
            InferenceItemDraft::ToolRequest {
                call_id,
                name,
                arguments_json,
                descriptor_fingerprint,
            } => {
                requests.push(PendingToolRequest {
                    turn_id: item.turn_id().clone(),
                    call_id: call_id.clone(),
                    name: name.clone(),
                    arguments_json: arguments_json.clone(),
                    descriptor_fingerprint: descriptor_fingerprint.clone(),
                });
            }
            InferenceItemDraft::ToolResult { call_id, .. } => {
                completed.insert(call_id.clone());
            }
            _ => {}
        }
    }
    let pending_tools = requests
        .into_iter()
        .filter(|request| !completed.contains(&request.call_id))
        .collect();
    let prepared_turn_without_output = state.manifests.last().and_then(|manifest| {
        let has_output = state.items.iter().any(|item| {
            item.turn_id() == manifest.turn_id()
                && !matches!(
                    item.payload(),
                    InferenceItemDraft::VisibleMessage {
                        role: VisibleMessageRole::User,
                        ..
                    }
                )
        });
        (!has_output).then(|| manifest.turn_id().clone())
    });
    ContextProjection {
        journal_revision: state.journal_revision,
        items: state.items,
        manifests: state.manifests,
        checkpoints: state.checkpoints,
        pending_tools,
        prepared_turn_without_output,
    }
}

fn replay(
    envelopes: Vec<autostudio_core::context::ContextEventEnvelope>,
    run_id: &AgentRunId,
) -> Result<ReplayState, ContextError> {
    let mut items = Vec::new();
    let mut manifests = Vec::new();
    let mut checkpoints = Vec::new();
    let mut expected_event_sequence = 1_u64;
    let mut expected_item_sequence = 1_u64;
    let mut tool_requests = std::collections::HashMap::new();
    let mut tool_results = std::collections::HashSet::new();
    for envelope in envelopes {
        if envelope.sequence() != expected_event_sequence {
            return Err(ContextError::InconsistentJournal(format!(
                "expected Context Event {expected_event_sequence}, found {}",
                envelope.sequence()
            )));
        }
        match envelope.event() {
            ContextEvent::InferenceItemAppended { item } => {
                validate_replayed_item(
                    item,
                    run_id,
                    expected_item_sequence,
                    &mut tool_requests,
                    &mut tool_results,
                )?;
                items.push(item.clone());
                expected_item_sequence = expected_item_sequence
                    .checked_add(1)
                    .ok_or(ContextError::SequenceExhausted)?;
            }
            ContextEvent::ContextPrepared { manifest } => {
                validate_replayed_manifest(
                    manifest,
                    run_id,
                    envelope.sequence(),
                    &items,
                    checkpoints.last(),
                )?;
                manifests.push(manifest.as_ref().clone());
            }
            ContextEvent::CompactionCommitted { checkpoint } => {
                validate_replayed_checkpoint(
                    checkpoint,
                    run_id,
                    envelope.sequence(),
                    &items,
                    checkpoints.last(),
                )?;
                checkpoints.push(checkpoint.as_ref().clone());
            }
        }
        expected_event_sequence = expected_event_sequence
            .checked_add(1)
            .ok_or(ContextError::SequenceExhausted)?;
    }
    Ok(ReplayState {
        journal_revision: expected_event_sequence - 1,
        next_item_sequence: expected_item_sequence,
        items,
        manifests,
        checkpoints,
    })
}

fn validate_replayed_item(
    item: &InferenceItem,
    run_id: &AgentRunId,
    expected_item_sequence: u64,
    tool_requests: &mut std::collections::HashMap<String, String>,
    tool_results: &mut std::collections::HashSet<String>,
) -> Result<(), ContextError> {
    if item.run_id() != run_id || item.sequence() != expected_item_sequence {
        return Err(ContextError::InconsistentJournal(
            "Inference Item Run or sequence does not match its journal".to_owned(),
        ));
    }
    item.payload().validate()?;
    if item.content_hash() != digest_json(item.payload())? {
        return Err(ContextError::InconsistentJournal(
            "Inference Item content hash does not match its payload".to_owned(),
        ));
    }
    match item.payload() {
        InferenceItemDraft::ToolRequest { call_id, name, .. } => {
            if tool_requests
                .insert(call_id.clone(), name.clone())
                .is_some()
            {
                return Err(ContextError::InconsistentJournal(
                    "duplicate Tool Request call id".to_owned(),
                ));
            }
        }
        InferenceItemDraft::ToolResult { call_id, name, .. } => {
            let Some(request_name) = tool_requests.get(call_id) else {
                return Err(ContextError::InconsistentJournal(
                    "orphan Tool Result".to_owned(),
                ));
            };
            if request_name != name || !tool_results.insert(call_id.clone()) {
                return Err(ContextError::InconsistentJournal(
                    "mismatched or duplicate Tool Result".to_owned(),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_replayed_manifest(
    manifest: &ContextManifest,
    run_id: &AgentRunId,
    event_sequence: u64,
    items: &[InferenceItem],
    checkpoint: Option<&CompactionCheckpoint>,
) -> Result<(), ContextError> {
    let surface_item_ids = current_surface_items(items, checkpoint)?
        .iter()
        .map(InferenceItem::id)
        .collect::<Vec<_>>();
    if manifest.run_id() != run_id
        || manifest.transcript_revision() != event_sequence - 1
        || manifest.included_item_ids().iter().ne(surface_item_ids)
    {
        return Err(ContextError::InconsistentJournal(
            "Context Manifest does not match its durable transcript".to_owned(),
        ));
    }
    if manifest.compaction_checkpoint() != checkpoint.map(CompactionCheckpoint::id) {
        return Err(ContextError::InconsistentJournal(
            "Context Manifest does not bind the latest compaction checkpoint".to_owned(),
        ));
    }
    manifest.validate()
}

fn validate_replayed_checkpoint(
    checkpoint: &CompactionCheckpoint,
    run_id: &AgentRunId,
    event_sequence: u64,
    items: &[InferenceItem],
    previous: Option<&CompactionCheckpoint>,
) -> Result<(), ContextError> {
    if checkpoint.run_id() != run_id || checkpoint.source_journal_revision() != event_sequence - 1 {
        return Err(ContextError::InconsistentJournal(
            "Compaction checkpoint does not match its Run and journal revision".to_owned(),
        ));
    }
    checkpoint
        .validate()
        .map_err(|error| ContextError::InconsistentJournal(error.to_string()))?;
    validate_compaction_cut(
        items,
        previous,
        checkpoint.replaces_item_ids(),
        checkpoint.first_kept_item_id(),
    )
}

fn current_surface_items<'a>(
    items: &'a [InferenceItem],
    checkpoint: Option<&CompactionCheckpoint>,
) -> Result<&'a [InferenceItem], ContextError> {
    let Some(checkpoint) = checkpoint else {
        return Ok(items);
    };
    let first_kept = items
        .iter()
        .position(|item| item.id() == checkpoint.first_kept_item_id())
        .ok_or_else(|| {
            ContextError::InconsistentJournal(
                "Compaction first kept item is missing from the Transcript".to_owned(),
            )
        })?;
    Ok(&items[first_kept..])
}

fn validate_compaction_cut(
    items: &[InferenceItem],
    previous: Option<&CompactionCheckpoint>,
    replaces_item_ids: &[autostudio_core::context::InferenceItemId],
    first_kept_item_id: &autostudio_core::context::InferenceItemId,
) -> Result<(), ContextError> {
    let first_kept = items
        .iter()
        .position(|item| item.id() == first_kept_item_id)
        .ok_or_else(|| {
            ContextError::InconsistentJournal(
                "Compaction first kept item is missing from the Transcript".to_owned(),
            )
        })?;
    if first_kept == 0
        || items[..first_kept]
            .iter()
            .map(InferenceItem::id)
            .ne(replaces_item_ids.iter())
    {
        return Err(ContextError::InconsistentJournal(
            "Compaction must replace one exact contiguous Transcript prefix".to_owned(),
        ));
    }
    if let Some(previous) = previous {
        let previous_first_kept = items
            .iter()
            .position(|item| item.id() == previous.first_kept_item_id())
            .ok_or_else(|| {
                ContextError::InconsistentJournal(
                    "Previous compaction cut point is missing from the Transcript".to_owned(),
                )
            })?;
        if first_kept <= previous_first_kept {
            return Err(ContextError::InconsistentJournal(
                "A repeated compaction must advance the kept Transcript tail".to_owned(),
            ));
        }
    }

    let mut request_positions = std::collections::HashMap::new();
    let mut result_positions = std::collections::HashMap::new();
    for (position, item) in items.iter().enumerate() {
        match item.payload() {
            InferenceItemDraft::ToolRequest { call_id, .. } => {
                request_positions.insert(call_id, position);
            }
            InferenceItemDraft::ToolResult { call_id, .. } => {
                result_positions.insert(call_id, position);
            }
            _ => {}
        }
    }
    for (call_id, request_position) in request_positions {
        if request_position < first_kept
            && result_positions
                .get(call_id)
                .is_none_or(|result_position| *result_position >= first_kept)
        {
            return Err(ContextError::InconsistentJournal(
                "Compaction cannot split or hide a pending Tool Request/Result pair".to_owned(),
            ));
        }
    }
    Ok(())
}

fn build_user_items(
    request: &PrepareContext,
    mut next_item_sequence: u64,
    created_at_unix_millis: u64,
) -> Result<Vec<InferenceItem>, ContextError> {
    let mut items = Vec::with_capacity(request.new_user_messages.len());
    for content in &request.new_user_messages {
        let payload = InferenceItemDraft::VisibleMessage {
            role: VisibleMessageRole::User,
            content: content.clone(),
        };
        let item = InferenceItem::new(
            request.run_id.clone(),
            request.turn_id.clone(),
            next_item_sequence,
            created_at_unix_millis,
            digest_json(&payload)?,
            payload,
        )?;
        next_item_sequence = next_item_sequence
            .checked_add(1)
            .ok_or(ContextError::SequenceExhausted)?;
        items.push(item);
    }
    Ok(items)
}

fn plan_context_retrieval(
    store: &dyn ContextEventStore,
    request: &PrepareContext,
    all_items: &[InferenceItem],
    checkpoint: Option<&CompactionCheckpoint>,
) -> Result<Option<ContextRetrievalSelection>, ContextError> {
    let Some(checkpoint) = checkpoint else {
        return Ok(None);
    };
    if request.new_user_messages.is_empty() {
        return Ok(None);
    }
    let surface_items = current_surface_items(all_items, Some(checkpoint))?;
    let mut excluded_item_ids = surface_items
        .iter()
        .map(|item| item.id().clone())
        .collect::<Vec<_>>();
    let summary = serde_json::to_string(checkpoint.summary())
        .map_err(|error| ContextError::Serialization(error.to_string()))?;
    for item in all_items {
        if (summary.contains(&item.id().as_str()) || summary.contains(item.content_hash()))
            && !excluded_item_ids.contains(item.id())
        {
            excluded_item_ids.push(item.id().clone());
        }
    }
    let query = ContextRetrievalQuery::new(
        request.run_id.clone(),
        Some(request.new_user_messages.join("\n")),
        Vec::new(),
        excluded_item_ids,
        ContextRetrievalReason::CurrentInputSimilarity,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
    )?;
    store.retrieve_context(&query).map_err(ContextError::from)
}

fn project_messages(
    items: &[InferenceItem],
    checkpoint: Option<&CompactionCheckpoint>,
    spills: &[ToolResultSpillReference],
    retrieval_selection: Option<&ContextRetrievalSelection>,
) -> Result<Vec<CanonicalMessage>, ContextError> {
    let mut messages = Vec::new();
    if let Some(checkpoint) = checkpoint {
        let content = serde_json::to_string(&serde_json::json!({
            "type": "auto_studio_context_summary",
            "source": {
                "sourceJournalRevision": checkpoint.source_journal_revision(),
                "contentHash": checkpoint.content_hash(),
            },
            "summary": checkpoint.summary(),
        }))
        .map_err(|error| ContextError::Serialization(error.to_string()))?;
        messages.push(CanonicalMessage::ContextSummary { content });
    }
    if let Some(selection) = retrieval_selection {
        for hit in selection.hits() {
            messages.push(CanonicalMessage::RetrievedContext {
                content: hit.model_visible_content(),
            });
        }
    }
    let mut assistant_turn: Option<InferenceTurnId> = None;
    for item in items {
        match item.payload() {
            InferenceItemDraft::VisibleMessage { role, content } => match role {
                VisibleMessageRole::User => {
                    assistant_turn = None;
                    messages.push(CanonicalMessage::User {
                        content: content.clone(),
                    });
                }
                VisibleMessageRole::Assistant => {
                    if assistant_turn.as_ref() == Some(item.turn_id())
                        && let Some(CanonicalMessage::Assistant {
                            content: current, ..
                        }) = messages.last_mut()
                    {
                        *current = Some(content.clone());
                    } else {
                        messages.push(CanonicalMessage::Assistant {
                            content: Some(content.clone()),
                            tool_calls: Vec::new(),
                        });
                        assistant_turn = Some(item.turn_id().clone());
                    }
                }
            },
            InferenceItemDraft::ToolRequest {
                call_id,
                name,
                arguments_json,
                ..
            } => {
                let call = CanonicalToolCall {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    arguments_json: arguments_json.clone(),
                };
                if assistant_turn.as_ref() == Some(item.turn_id())
                    && let Some(CanonicalMessage::Assistant { tool_calls, .. }) =
                        messages.last_mut()
                {
                    tool_calls.push(call);
                } else {
                    messages.push(CanonicalMessage::Assistant {
                        content: None,
                        tool_calls: vec![call],
                    });
                    assistant_turn = Some(item.turn_id().clone());
                }
            }
            InferenceItemDraft::ToolResult {
                call_id,
                name,
                content,
                is_error,
                ..
            } => {
                assistant_turn = None;
                let content = spills
                    .iter()
                    .find(|spill| spill.item_id() == item.id())
                    .map_or_else(|| Ok(content.clone()), model_visible_spill_reference)?;
                messages.push(CanonicalMessage::Tool {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    content,
                    is_error: *is_error,
                });
            }
            InferenceItemDraft::Usage { .. } | InferenceItemDraft::Finish { .. } => {}
        }
    }
    Ok(messages)
}

struct PreparedSurfaceArtifacts {
    messages: Vec<CanonicalMessage>,
    spills: Vec<ContextSpillBlob>,
    metrics: ContextSurfaceMetrics,
}

struct SelectedPreparedSurface {
    new_checkpoint: Option<CompactionCheckpoint>,
    surface_start: usize,
    prepared_surface: PreparedSurfaceArtifacts,
    preparation_reason: ContextPreparationReason,
    source_journal_revision: u64,
}

struct AutomaticCompaction {
    checkpoint: CompactionCheckpoint,
    surface_start: usize,
    surface: PreparedSurfaceArtifacts,
}

fn select_prepared_surface(
    request: &PrepareContext,
    state: &ReplayState,
    all_items: &[InferenceItem],
    new_item_count: usize,
    retrieval_selection: Option<&ContextRetrievalSelection>,
    created_at_unix_millis: u64,
) -> Result<SelectedPreparedSurface, ContextError> {
    let provider_overflow_recovery = state.items.last().is_some_and(|item| {
        matches!(
            item.payload(),
            InferenceItemDraft::Finish {
                reason: autostudio_core::context::InferenceFinishReason::ContextOverflow,
                ..
            }
        )
    });
    if provider_overflow_recovery
        && state.manifests.iter().any(|manifest| {
            manifest.preparation_reason() == ContextPreparationReason::ProviderOverflowRecovery
        })
    {
        return Err(ContextError::OverflowRecoveryExhausted);
    }
    let previous_checkpoint = state.checkpoints.last();
    let previous_surface_items = current_surface_items(all_items, previous_checkpoint)?;
    let standard_surface = prepare_surface(
        request,
        previous_surface_items,
        previous_checkpoint,
        None,
        false,
        retrieval_selection,
    )?;
    let preparation_reason = if provider_overflow_recovery {
        ContextPreparationReason::ProviderOverflowRecovery
    } else if matches!(
        standard_surface.metrics.prepared_footprint().pressure(),
        ContextPressure::Hard | ContextPressure::Overflow
    ) {
        ContextPreparationReason::PressureCompaction
    } else {
        ContextPreparationReason::Standard
    };
    let source_journal_revision = state
        .journal_revision
        .checked_add(u64::try_from(new_item_count).map_err(|_| ContextError::SequenceExhausted)?)
        .ok_or(ContextError::SequenceExhausted)?;
    if preparation_reason == ContextPreparationReason::Standard {
        reject_hard_pressure(standard_surface.metrics.prepared_footprint())?;
        return Ok(SelectedPreparedSurface {
            new_checkpoint: None,
            surface_start: all_items.len() - previous_surface_items.len(),
            prepared_surface: standard_surface,
            preparation_reason,
            source_journal_revision,
        });
    }
    let compacted = automatic_compaction(
        request,
        all_items,
        new_item_count,
        previous_checkpoint,
        source_journal_revision,
        standard_surface.metrics.initial_footprint(),
        retrieval_selection,
        created_at_unix_millis,
    )?;
    Ok(SelectedPreparedSurface {
        new_checkpoint: Some(compacted.checkpoint),
        surface_start: compacted.surface_start,
        prepared_surface: compacted.surface,
        preparation_reason,
        source_journal_revision,
    })
}

fn prepare_surface(
    request: &PrepareContext,
    items: &[InferenceItem],
    checkpoint: Option<&CompactionCheckpoint>,
    initial_footprint: Option<ContextFootprint>,
    compaction_applied: bool,
    retrieval_selection: Option<&ContextRetrievalSelection>,
) -> Result<PreparedSurfaceArtifacts, ContextError> {
    let initial_messages = project_messages(items, checkpoint, &[], retrieval_selection)?;
    let initial_footprint = initial_footprint.map_or_else(
        || {
            ContextFootprint::measure(
                &request.instructions,
                &initial_messages,
                &request.tools,
                request.continuity_overhead_tokens,
                &request.token_budget,
            )
        },
        Ok,
    )?;
    let (spills, spill_references) = plan_tool_result_spills(items)?;
    let messages = if spill_references.is_empty() {
        initial_messages
    } else {
        project_messages(items, checkpoint, &spill_references, retrieval_selection)?
    };
    let prepared_footprint = ContextFootprint::measure(
        &request.instructions,
        &messages,
        &request.tools,
        request.continuity_overhead_tokens,
        &request.token_budget,
    )?;
    Ok(PreparedSurfaceArtifacts {
        messages,
        spills,
        metrics: ContextSurfaceMetrics::new(
            initial_footprint,
            prepared_footprint,
            spill_references,
            compaction_applied,
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn automatic_compaction(
    request: &PrepareContext,
    all_items: &[InferenceItem],
    new_item_count: usize,
    previous: Option<&CompactionCheckpoint>,
    source_journal_revision: u64,
    initial_footprint: &ContextFootprint,
    retrieval_selection: Option<&ContextRetrievalSelection>,
    created_at_unix_millis: u64,
) -> Result<AutomaticCompaction, ContextError> {
    let minimum_cut = previous
        .and_then(|checkpoint| {
            all_items
                .iter()
                .position(|item| item.id() == checkpoint.first_kept_item_id())
        })
        .map_or(1, |position| position.saturating_add(1));
    let protected_turn_start = recent_turn_tail_start(all_items);
    let new_items_start = all_items.len().saturating_sub(new_item_count);
    let maximum_cut = protected_turn_start.min(new_items_start);
    if minimum_cut > maximum_cut {
        return Err(ContextError::AutomaticCompactionUnavailable);
    }

    for cut in minimum_cut..=maximum_cut {
        if all_items[cut - 1].turn_id() == all_items[cut].turn_id() {
            continue;
        }
        let replaces_item_ids = all_items[..cut]
            .iter()
            .map(|item| item.id().clone())
            .collect::<Vec<_>>();
        let first_kept_item_id = all_items[cut].id().clone();
        if validate_compaction_cut(all_items, previous, &replaces_item_ids, &first_kept_item_id)
            .is_err()
        {
            continue;
        }
        let summary = summarize_replaced_prefix(&all_items[..cut])?;
        let checkpoint = CompactionCheckpoint::new(
            request.run_id.clone(),
            source_journal_revision,
            replaces_item_ids,
            first_kept_item_id,
            summary,
            created_at_unix_millis,
        )
        .map_err(|error| ContextError::InconsistentJournal(error.to_string()))?;
        let surface = prepare_surface(
            request,
            &all_items[cut..],
            Some(&checkpoint),
            Some(initial_footprint.clone()),
            true,
            retrieval_selection,
        )?;
        if compaction_recovers_surface(
            surface.metrics.initial_footprint(),
            surface.metrics.prepared_footprint(),
        ) {
            return Ok(AutomaticCompaction {
                checkpoint,
                surface_start: cut,
                surface,
            });
        }
    }
    Err(ContextError::AutomaticCompactionUnavailable)
}

fn recent_turn_tail_start(items: &[InferenceItem]) -> usize {
    let mut turns = std::collections::HashSet::new();
    let mut protected_start = items.len();
    for (position, item) in items.iter().enumerate().rev() {
        if !turns.contains(item.turn_id()) && turns.len() == CONTEXT_COMPACTION_MIN_RECENT_TURNS {
            break;
        }
        turns.insert(item.turn_id().clone());
        protected_start = position;
    }
    protected_start
}

fn compaction_recovers_surface(initial: &ContextFootprint, prepared: &ContextFootprint) -> bool {
    if prepared.total_serialized_bytes() >= initial.total_serialized_bytes() {
        return false;
    }
    match prepared.pressure() {
        ContextPressure::Normal => true,
        ContextPressure::Unknown => {
            u128::from(prepared.estimated_input_tokens()) * 100
                <= u128::from(initial.estimated_input_tokens())
                    * u128::from(CONTEXT_COMPACTION_UNKNOWN_TARGET_PERCENT)
        }
        ContextPressure::Soft | ContextPressure::Hard | ContextPressure::Overflow => false,
    }
}

fn summarize_replaced_prefix(
    items: &[InferenceItem],
) -> Result<StructuredRunSummary, ContextError> {
    let objective_source = items
        .iter()
        .find_map(|item| match item.payload() {
            InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::User,
                content,
            } => Some(content.as_str()),
            _ => None,
        })
        .ok_or(ContextError::AutomaticCompactionUnavailable)?;
    let objective = summarize_objective(objective_source);
    let constraints = extract_constraints(objective_source);
    let mut seen_objective = false;
    let mut creator_decisions = Vec::new();
    let mut completed_work = Vec::new();
    let mut open_items = Vec::new();
    let mut artifact_references = Vec::new();
    for item in items {
        match item.payload() {
            InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::User,
                content,
            } => {
                if seen_objective {
                    push_summary_item(
                        &mut creator_decisions,
                        format_summary_item(item, "creator", content),
                    );
                }
                seen_objective = true;
            }
            InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::Assistant,
                content,
            } => push_summary_item(
                &mut completed_work,
                format_summary_item(item, "assistant", content),
            ),
            InferenceItemDraft::ToolResult {
                name,
                content,
                is_error,
                execution_id,
                ..
            } => {
                let (status, target) = if *is_error {
                    ("error", &mut open_items)
                } else {
                    ("ok", &mut completed_work)
                };
                push_summary_item(
                    target,
                    format_summary_item(item, &format!("tool:{name}:{status}"), content),
                );
                if let Some(execution_id) = execution_id {
                    push_summary_item(
                        &mut artifact_references,
                        bounded_text(
                            &format!("execution:{execution_id} sourceItem:{}", item.id().as_str()),
                            CONTEXT_COMPACTION_SUMMARY_ITEM_CHARS,
                        ),
                    );
                }
            }
            InferenceItemDraft::ToolRequest { .. }
            | InferenceItemDraft::Usage { .. }
            | InferenceItemDraft::Finish { .. } => {}
        }
    }
    StructuredRunSummary::new(StructuredRunSummaryDraft {
        objective,
        creator_decisions,
        constraints,
        completed_work,
        open_items,
        artifact_references,
    })
    .map_err(|error| ContextError::InconsistentJournal(error.to_string()))
}

fn summarize_objective(source: &str) -> String {
    let summary = serde_json::from_str::<serde_json::Value>(source)
        .ok()
        .and_then(|value| {
            value
                .get("summary")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| source.to_owned());
    bounded_text(&summary, CONTEXT_COMPACTION_SUMMARY_OBJECTIVE_CHARS)
}

fn extract_constraints(objective: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(objective) else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    for field in ["purpose", "targetDurationSeconds"] {
        if let Some(value) = value.get(field).filter(|value| !value.is_null()) {
            push_summary_item(
                &mut facts,
                bounded_text(
                    &format!("{field}:{}", json_fact(value)),
                    CONTEXT_COMPACTION_SUMMARY_ITEM_CHARS,
                ),
            );
        }
    }
    for field in ["style", "mood", "instrumentation", "constraints"] {
        for value in value
            .get(field)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            push_summary_item(
                &mut facts,
                bounded_text(
                    &format!("{field}:{}", json_fact(value)),
                    CONTEXT_COMPACTION_SUMMARY_ITEM_CHARS,
                ),
            );
        }
    }
    facts
}

fn json_fact(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn format_summary_item(item: &InferenceItem, kind: &str, content: &str) -> String {
    bounded_text(
        &format!(
            "{kind} sourceItem:{} sourceHash:{} content:{}",
            item.id().as_str(),
            item.content_hash(),
            content
        ),
        CONTEXT_COMPACTION_SUMMARY_ITEM_CHARS,
    )
}

fn push_summary_item(items: &mut Vec<String>, value: String) {
    if items.len() < CONTEXT_COMPACTION_SUMMARY_MAX_ITEMS && !value.trim().is_empty() {
        items.push(value);
    }
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let mut bounded = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

fn plan_tool_result_spills(
    items: &[InferenceItem],
) -> Result<(Vec<ContextSpillBlob>, Vec<ToolResultSpillReference>), ContextError> {
    let mut blobs = Vec::new();
    let mut references = Vec::new();
    for item in items {
        let InferenceItemDraft::ToolResult { content, .. } = item.payload() else {
            continue;
        };
        if content.len() <= CONTEXT_TOOL_RESULT_SPILL_THRESHOLD_BYTES {
            continue;
        }
        let blob = ContextSpillBlob::new(content.clone())?;
        let preview = content
            .chars()
            .take(CONTEXT_TOOL_RESULT_PREVIEW_CHARS)
            .collect();
        let reference = ToolResultSpillReference::new(item.id().clone(), &blob, preview)?;
        blobs.push(blob);
        references.push(reference);
    }
    Ok((blobs, references))
}

fn model_visible_spill_reference(spill: &ToolResultSpillReference) -> Result<String, ContextError> {
    serde_json::to_string(&serde_json::json!({
        "type": "auto_studio_spilled_tool_result",
        "sourceItemId": spill.item_id().as_str(),
        "contentHash": spill.content_hash(),
        "originalBytes": spill.original_bytes(),
        "preview": spill.retained_preview(),
        "recovery": {
            "kind": "inference_item",
            "itemId": spill.item_id().as_str(),
        },
    }))
    .map_err(|error| ContextError::Serialization(error.to_string()))
}

fn reject_hard_pressure(footprint: &ContextFootprint) -> Result<(), ContextError> {
    if matches!(
        footprint.pressure(),
        ContextPressure::Hard | ContextPressure::Overflow
    ) {
        return Err(ContextError::CompactionRequired {
            estimated_tokens: footprint.estimated_input_tokens(),
            input_budget_tokens: footprint
                .input_budget_tokens()
                .expect("hard pressure requires a known input budget"),
        });
    }
    Ok(())
}

fn validate_record_binding<'a>(
    state: &'a ReplayState,
    request: &RecordInferenceTurn,
) -> Result<&'a ContextManifest, ContextError> {
    if state.journal_revision != request.expected_journal_revision {
        return Err(
            autostudio_core::context::ContextStoreError::RevisionConflict {
                expected: request.expected_journal_revision,
                actual: state.journal_revision,
            }
            .into(),
        );
    }
    let manifest = state
        .manifests
        .iter()
        .find(|manifest| manifest.context_id() == &request.context_id)
        .ok_or_else(|| {
            ContextError::InconsistentJournal(
                "Inference output refers to an unknown Context Manifest".to_owned(),
            )
        })?;
    if manifest.run_id() != &request.run_id || manifest.turn_id() != &request.turn_id {
        return Err(ContextError::InconsistentJournal(
            "Inference output does not match its Run and Turn binding".to_owned(),
        ));
    }
    if state.items.iter().any(|item| {
        item.turn_id() == &request.turn_id
            && !matches!(
                item.payload(),
                InferenceItemDraft::VisibleMessage {
                    role: VisibleMessageRole::User,
                    ..
                }
            )
    }) {
        return Err(ContextError::InconsistentJournal(
            "Inference Turn output has already been recorded".to_owned(),
        ));
    }
    Ok(manifest)
}

fn validate_provider_payload(
    payload: &InferenceItemDraft,
    manifest: &ContextManifest,
    call_ids: &mut std::collections::HashSet<String>,
) -> Result<(), ContextError> {
    let InferenceItemDraft::ToolRequest {
        call_id,
        name,
        descriptor_fingerprint,
        ..
    } = payload
    else {
        return Ok(());
    };
    if !call_ids.insert(call_id.clone()) {
        return Err(ContextError::InconsistentJournal(
            "Tool Request call id is already present in the Run".to_owned(),
        ));
    }
    let definition = manifest
        .tools()
        .iter()
        .find(|tool| tool.name == *name)
        .ok_or_else(|| {
            ContextError::InconsistentJournal(
                "Provider requested a Tool outside the prepared catalog".to_owned(),
            )
        })?;
    if definition.descriptor_fingerprint != *descriptor_fingerprint {
        return Err(ContextError::InconsistentJournal(
            "Tool Request fingerprint does not match the prepared catalog".to_owned(),
        ));
    }
    Ok(())
}

fn validate_prepare_request(request: &PrepareContext) -> Result<(), ContextError> {
    if request.project_id.trim().is_empty() {
        return Err(ContextError::EmptyField("prepare.project_id"));
    }
    if request.instructions.trim().is_empty() {
        return Err(ContextError::EmptyField("prepare.instructions"));
    }
    if request
        .new_user_messages
        .iter()
        .any(|message| message.trim().is_empty())
    {
        return Err(ContextError::EmptyField("prepare.user_message"));
    }
    request.provider_binding.validate()
}

fn validate_turn_items(items: &[InferenceItemDraft]) -> Result<(), ContextError> {
    let mut visible_count = 0_u8;
    let mut usage_count = 0_u8;
    let mut finish_count = 0_u8;
    let mut has_surface_output = false;
    for (index, item) in items.iter().enumerate() {
        match item {
            InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::Assistant,
                ..
            } => {
                visible_count = visible_count.saturating_add(1);
                has_surface_output = true;
            }
            InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::User,
                ..
            } => {
                return Err(ContextError::InconsistentJournal(
                    "Provider output cannot append a Creator message".to_owned(),
                ));
            }
            InferenceItemDraft::ToolRequest { .. } => has_surface_output = true,
            InferenceItemDraft::ToolResult { .. } => {
                return Err(ContextError::InconsistentJournal(
                    "Tool Results must be recorded through record_tool_results".to_owned(),
                ));
            }
            InferenceItemDraft::Usage { .. } => usage_count = usage_count.saturating_add(1),
            InferenceItemDraft::Finish { reason, .. } => {
                finish_count = finish_count.saturating_add(1);
                if index + 1 != items.len() {
                    return Err(ContextError::InconsistentJournal(
                        "Inference Finish must be the final item in its Turn".to_owned(),
                    ));
                }
                if *reason == autostudio_core::context::InferenceFinishReason::Completed
                    && !has_surface_output
                {
                    return Err(ContextError::InconsistentJournal(
                        "a completed Inference Turn must contain visible text or a Tool Request"
                            .to_owned(),
                    ));
                }
            }
        }
    }
    if visible_count > 1 || usage_count > 1 || finish_count != 1 {
        return Err(ContextError::InconsistentJournal(
            "Inference Turn has duplicate visible, usage, or finish items".to_owned(),
        ));
    }
    Ok(())
}

fn digest_json<T: Serialize>(value: &T) -> Result<String, ContextError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| ContextError::Serialization(error.to_string()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn now_unix_millis() -> Result<u64, ContextError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ContextError::InvalidClock)?
        .as_millis();
    u64::try_from(millis).map_err(|_| ContextError::InvalidClock)
}

#[derive(Serialize)]
struct ContextHashInput<'a> {
    project_id: &'a str,
    project_revision: u64,
    instructions: &'a str,
    messages: &'a [CanonicalMessage],
    tools: &'a [CanonicalToolDefinition],
    provider_binding: &'a ProviderBinding,
    compaction_checkpoint_hash: Option<&'a str>,
    surface_metrics: &'a ContextSurfaceMetrics,
    preparation_reason: ContextPreparationReason,
    retrieval_selection: Option<&'a ContextRetrievalSelection>,
    continuity_reference: Option<&'a ContinuityReference>,
    token_budget: &'a TokenBudgetPlan,
}
