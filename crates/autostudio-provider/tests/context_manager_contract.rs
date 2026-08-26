use std::sync::Arc;

use autostudio_core::agent::{AgentRunId, InferenceUsage};
use autostudio_core::compaction::{StructuredRunSummary, StructuredRunSummaryDraft};
use autostudio_core::context::{
    CanonicalMessage, CanonicalToolDefinition, ContextEventStore, ContextStoreError,
    InferenceFinishReason, InferenceItemDraft, InferenceTurnId, ProviderBinding, TokenBudgetPlan,
    VisibleMessageRole,
};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::context::{
    CommitCompaction, CompletedToolResult, ContextManager, PrepareContext, RecordInferenceTurn,
    RecordToolResults, fingerprint_tool_catalog,
};
use sha2::{Digest, Sha256};

#[test]
#[allow(clippy::too_many_lines)]
fn context_survives_reopen_and_reconstructs_a_second_turn_without_private_reasoning() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("context.autostudio");
    let run_id = AgentRunId::new();
    let first_turn_id = InferenceTurnId::new();
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("context store"));
    let manager = ContextManager::new(store.clone());
    let first = manager
        .prepare_turn(prepare(
            run_id.clone(),
            first_turn_id.clone(),
            1,
            "Compose an eight-bar piano idea",
        ))
        .expect("prepare first turn");
    assert_eq!(first.journal_revision(), 2);
    let descriptor_fingerprint = empty_digest();
    let recorded = manager
        .record_turn(RecordInferenceTurn {
            run_id: run_id.clone(),
            turn_id: first_turn_id,
            context_id: first.manifest().context_id().clone(),
            expected_journal_revision: first.journal_revision(),
            items: vec![
                InferenceItemDraft::VisibleMessage {
                    role: VisibleMessageRole::Assistant,
                    content: "I will add a piano motif and inspect the result.".to_owned(),
                },
                InferenceItemDraft::ToolRequest {
                    call_id: "call-1".to_owned(),
                    name: "project_add_midi_notes".to_owned(),
                    arguments_json: r#"{"track":"piano"}"#.to_owned(),
                    descriptor_fingerprint,
                },
                InferenceItemDraft::Usage {
                    usage: InferenceUsage {
                        input_tokens: Some(100),
                        output_tokens: Some(40),
                        actual_cost_minor_units: None,
                        currency: None,
                    },
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: None,
                },
            ],
        })
        .expect("record first turn");
    assert_eq!(recorded.journal_revision, 6);
    let recorded = manager
        .record_tool_results(RecordToolResults {
            run_id: run_id.clone(),
            expected_journal_revision: recorded.journal_revision,
            results: vec![CompletedToolResult {
                call_id: "call-1".to_owned(),
                name: "project_add_midi_notes".to_owned(),
                content: r#"{"notesAdded":16}"#.to_owned(),
                is_error: false,
                execution_id: Some("execution-1".to_owned()),
            }],
        })
        .expect("record Tool Result");
    assert_eq!(recorded.journal_revision, 7);
    let durable_json =
        serde_json::to_string(&store.context_events(&run_id).expect("events")).expect("event JSON");
    assert!(!durable_json.contains("private_reasoning"));
    assert!(!durable_json.contains("chain_of_thought"));
    drop(manager);
    drop(store);

    let reopened = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("reopened context store"),
    );
    let manager = ContextManager::new(reopened);
    let second_turn_id = InferenceTurnId::new();
    let second = manager
        .prepare_turn(prepare(
            run_id.clone(),
            second_turn_id.clone(),
            2,
            "Make the ending less predictable",
        ))
        .expect("prepare second turn after restart");
    assert_eq!(second.journal_revision(), 9);
    assert_eq!(second.messages().len(), 4);
    assert!(matches!(
        second.messages().first(),
        Some(CanonicalMessage::User { content }) if content.contains("eight-bar")
    ));
    assert!(matches!(
        second.messages().last(),
        Some(CanonicalMessage::User { content }) if content.contains("less predictable")
    ));
    assert!(second.messages().iter().any(|message| matches!(
        message,
        CanonicalMessage::Tool { call_id, .. } if call_id == "call-1"
    )));

    let error = manager
        .record_turn(RecordInferenceTurn {
            run_id,
            turn_id: second_turn_id,
            context_id: second.manifest().context_id().clone(),
            expected_journal_revision: 7,
            items: vec![InferenceItemDraft::Finish {
                reason: InferenceFinishReason::Interrupted,
                detail: None,
            }],
        })
        .expect_err("stale writer must be rejected");
    assert!(matches!(
        error,
        autostudio_core::context::ContextError::Store(ContextStoreError::RevisionConflict {
            expected: 7,
            actual: 9
        })
    ));
}

#[test]
fn tool_results_cannot_be_recorded_without_a_matching_pending_request() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("orphan.autostudio"))
            .expect("context store"),
    );
    let manager = ContextManager::new(store);
    let run_id = AgentRunId::new();
    let prepared = manager
        .prepare_turn(prepare(
            run_id.clone(),
            InferenceTurnId::new(),
            1,
            "Inspect the project",
        ))
        .expect("prepared Context");
    let error = manager
        .record_tool_results(RecordToolResults {
            run_id,
            expected_journal_revision: prepared.journal_revision(),
            results: vec![CompletedToolResult {
                call_id: "orphan-call".to_owned(),
                name: "project_add_midi_notes".to_owned(),
                content: "{\"ok\":true}".to_owned(),
                is_error: false,
                execution_id: Some("orphan-execution".to_owned()),
            }],
        })
        .expect_err("orphan Tool Result must be rejected");
    assert!(matches!(
        error,
        autostudio_core::context::ContextError::InconsistentJournal(_)
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn committed_compaction_keeps_the_full_transcript_and_rebuilds_a_bounded_surface_after_restart() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("compaction.autostudio");
    let run_id = AgentRunId::new();
    let first_turn_id = InferenceTurnId::new();
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("context store"));
    let manager = ContextManager::new(store.clone());
    let first = manager
        .prepare_turn(prepare(
            run_id.clone(),
            first_turn_id.clone(),
            1,
            "Compose an eight-bar cue without vocals",
        ))
        .expect("first Context");
    let recorded = manager
        .record_turn(RecordInferenceTurn {
            run_id: run_id.clone(),
            turn_id: first_turn_id,
            context_id: first.manifest().context_id().clone(),
            expected_journal_revision: first.journal_revision(),
            items: vec![
                InferenceItemDraft::VisibleMessage {
                    role: VisibleMessageRole::Assistant,
                    content: "I will inspect the piano region.".to_owned(),
                },
                InferenceItemDraft::ToolRequest {
                    call_id: "compact-call-1".to_owned(),
                    name: "project_add_midi_notes".to_owned(),
                    arguments_json: r#"{"track":"piano"}"#.to_owned(),
                    descriptor_fingerprint: empty_digest(),
                },
                InferenceItemDraft::Usage {
                    usage: InferenceUsage::default(),
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: None,
                },
            ],
        })
        .expect("first output");
    manager
        .record_tool_results(RecordToolResults {
            run_id: run_id.clone(),
            expected_journal_revision: recorded.journal_revision,
            results: vec![CompletedToolResult {
                call_id: "compact-call-1".to_owned(),
                name: "project_add_midi_notes".to_owned(),
                content: r#"{"notesAdded":16}"#.to_owned(),
                is_error: false,
                execution_id: Some("compact-execution-1".to_owned()),
            }],
        })
        .expect("Tool Result");

    let projection = manager.inspect_run(&run_id).expect("projection");
    assert_eq!(projection.journal_revision(), 7);
    assert_eq!(projection.items().len(), 6);
    let item_ids = projection
        .items()
        .iter()
        .map(|item| item.id().clone())
        .collect::<Vec<_>>();
    let split_error = manager
        .commit_compaction(CommitCompaction {
            run_id: run_id.clone(),
            expected_journal_revision: 7,
            replaces_item_ids: item_ids[..3].to_vec(),
            first_kept_item_id: item_ids[3].clone(),
            summary: compaction_summary(),
        })
        .expect_err("Tool Request and Result cannot be split");
    assert!(split_error.to_string().contains("Tool Request/Result"));

    let committed = manager
        .commit_compaction(CommitCompaction {
            run_id: run_id.clone(),
            expected_journal_revision: 7,
            replaces_item_ids: item_ids[..2].to_vec(),
            first_kept_item_id: item_ids[2].clone(),
            summary: compaction_summary(),
        })
        .expect("valid compaction");
    assert_eq!(committed.journal_revision, 8);
    drop(manager);
    drop(store);

    let reopened = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("reopened context store"),
    );
    let manager = ContextManager::new(reopened);
    let projection = manager.inspect_run(&run_id).expect("replayed projection");
    assert_eq!(
        projection.items().len(),
        6,
        "Transcript must remain complete"
    );
    assert_eq!(projection.checkpoints().len(), 1);
    assert_eq!(
        projection.checkpoints()[0].content_hash(),
        committed.checkpoint.content_hash()
    );

    let second_turn_id = InferenceTurnId::new();
    let second = manager
        .prepare_turn(prepare(
            run_id.clone(),
            second_turn_id.clone(),
            2,
            "Make the ending less predictable",
        ))
        .expect("Context from checkpoint and kept tail");
    assert_eq!(second.journal_revision(), 10);
    assert_eq!(second.messages().len(), 4);
    assert!(matches!(
        second.messages().first(),
        Some(CanonicalMessage::ContextSummary { content })
            if content.contains("eight-bar cue") && content.contains("context_summary")
    ));
    assert!(second.messages().iter().any(|message| matches!(
        message,
        CanonicalMessage::Tool { call_id, .. } if call_id == "compact-call-1"
    )));
    assert!(matches!(
        second.messages().last(),
        Some(CanonicalMessage::User { content }) if content.contains("less predictable")
    ));
    assert_eq!(second.manifest().included_item_ids().len(), 5);
    assert_eq!(
        second.manifest().compaction_checkpoint(),
        Some(committed.checkpoint.id())
    );

    let repeated_error = manager
        .commit_compaction(CommitCompaction {
            run_id: run_id.clone(),
            expected_journal_revision: second.journal_revision(),
            replaces_item_ids: item_ids[..2].to_vec(),
            first_kept_item_id: item_ids[2].clone(),
            summary: compaction_summary(),
        })
        .expect_err("a repeated checkpoint must advance its cut");
    assert!(repeated_error.to_string().contains("must advance"));

    let second_recorded = manager
        .record_turn(RecordInferenceTurn {
            run_id: run_id.clone(),
            turn_id: second_turn_id,
            context_id: second.manifest().context_id().clone(),
            expected_journal_revision: second.journal_revision(),
            items: vec![
                InferenceItemDraft::VisibleMessage {
                    role: VisibleMessageRole::Assistant,
                    content: "I revised the ending while keeping the established cue.".to_owned(),
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: None,
                },
            ],
        })
        .expect("second output");
    let projection = manager.inspect_run(&run_id).expect("updated projection");
    assert_eq!(projection.items().len(), 9);
    let updated_item_ids = projection
        .items()
        .iter()
        .map(|item| item.id().clone())
        .collect::<Vec<_>>();
    let repeated = manager
        .commit_compaction(CommitCompaction {
            run_id: run_id.clone(),
            expected_journal_revision: second_recorded.journal_revision,
            replaces_item_ids: updated_item_ids[..6].to_vec(),
            first_kept_item_id: updated_item_ids[6].clone(),
            summary: compaction_summary(),
        })
        .expect("advancing repeated compaction");
    assert_eq!(repeated.journal_revision, 13);
    let projection = manager
        .inspect_run(&run_id)
        .expect("twice compacted projection");
    assert_eq!(
        projection.items().len(),
        9,
        "Transcript must remain complete"
    );
    assert_eq!(projection.checkpoints().len(), 2);
}

fn prepare(
    run_id: AgentRunId,
    turn_id: InferenceTurnId,
    project_revision: u64,
    message: &str,
) -> PrepareContext {
    let tools =
        vec![CanonicalToolDefinition::new(
        "project_add_midi_notes",
        "Add MIDI notes to one Project track",
        r#"{"type":"object","properties":{"track":{"type":"string"}},"required":["track"]}"#,
        empty_digest(),
    )
    .expect("Tool definition")];
    PrepareContext {
        run_id,
        turn_id,
        project_id: "context-contract-project".to_owned(),
        project_revision,
        instructions: "Act as a professional music production agent.".to_owned(),
        new_user_messages: vec![message.to_owned()],
        provider_binding: ProviderBinding {
            provider_kind: "test-provider".to_owned(),
            model: "test-model".to_owned(),
            protocol: "test-protocol".to_owned(),
            thinking_level: ThinkingLevel::High,
            thinking_control: ThinkingControl::Effort,
            thinking_budget_tokens: None,
            capability_revision: "test-capability/1".to_owned(),
            mapping_revision: "test-mapping/1".to_owned(),
            tool_catalog_fingerprint: fingerprint_tool_catalog(&tools),
        },
        continuity_reference: None,
        tools,
        token_budget: TokenBudgetPlan::known(32_768, 4_096, 1_024).expect("valid token budget"),
    }
}

fn empty_digest() -> String {
    format!("sha256:{:x}", Sha256::digest([]))
}

fn compaction_summary() -> StructuredRunSummary {
    StructuredRunSummary::new(StructuredRunSummaryDraft {
        objective: "Create an editable eight-bar cue".to_owned(),
        creator_decisions: vec!["Keep the piano direction".to_owned()],
        constraints: vec!["No vocals".to_owned()],
        completed_work: vec!["Project inspection completed".to_owned()],
        open_items: vec!["Revise the ending".to_owned()],
        artifact_references: vec!["artifact:preview-1".to_owned()],
    })
    .expect("summary")
}
