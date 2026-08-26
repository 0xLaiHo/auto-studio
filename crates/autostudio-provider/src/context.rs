//! Run-scoped context assembly and durable transcript recording.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use autostudio_core::agent::AgentRunId;
use autostudio_core::compaction::{CompactionCheckpoint, StructuredRunSummary};
use autostudio_core::context::{
    CanonicalMessage, CanonicalToolCall, CanonicalToolDefinition, ContextError, ContextEvent,
    ContextEventStore, ContextId, ContextManifest, InferenceItem, InferenceItemDraft,
    InferenceTurnId, PreparedContext, ProviderBinding, TokenBudgetPlan, VisibleMessageRole,
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
        let mut items = Vec::with_capacity(request.new_user_messages.len());
        let mut next_item_sequence = state.next_item_sequence;
        for content in &request.new_user_messages {
            let payload = InferenceItemDraft::VisibleMessage {
                role: VisibleMessageRole::User,
                content: content.clone(),
            };
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

        let mut all_items = state.items;
        all_items.extend(items.iter().cloned());
        let checkpoint = state.checkpoints.last();
        let surface_items = current_surface_items(&all_items, checkpoint)?;
        let messages = project_messages(surface_items, checkpoint)?;
        let included_item_ids = surface_items.iter().map(|item| item.id().clone()).collect();
        let transcript_revision = state
            .journal_revision
            .checked_add(u64::try_from(items.len()).map_err(|_| ContextError::SequenceExhausted)?)
            .ok_or(ContextError::SequenceExhausted)?;
        let content_hash = digest_json(&ContextHashInput {
            project_id: &request.project_id,
            project_revision: request.project_revision,
            instructions: &request.instructions,
            messages: &messages,
            tools: &request.tools,
            provider_binding: &request.provider_binding,
            compaction_checkpoint_hash: checkpoint.map(CompactionCheckpoint::content_hash),
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
            request.continuity_reference,
            request.token_budget,
            content_hash,
        )?;
        let mut events = items
            .iter()
            .cloned()
            .map(|item| ContextEvent::InferenceItemAppended { item })
            .collect::<Vec<_>>();
        events.push(ContextEvent::ContextPrepared {
            manifest: Box::new(manifest.clone()),
        });
        let journal_revision =
            self.store
                .append_context_events(&request.run_id, state.journal_revision, &events)?;
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

fn project_messages(
    items: &[InferenceItem],
    checkpoint: Option<&CompactionCheckpoint>,
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
                messages.push(CanonicalMessage::Tool {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                });
            }
            InferenceItemDraft::Usage { .. } | InferenceItemDraft::Finish { .. } => {}
        }
    }
    Ok(messages)
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
    continuity_reference: Option<&'a ContinuityReference>,
    token_budget: &'a TokenBudgetPlan,
}
