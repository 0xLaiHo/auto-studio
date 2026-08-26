use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use autostudio_core::agent::{AgentRunId, InferenceUsage};
use autostudio_core::compaction::{StructuredRunSummary, StructuredRunSummaryDraft};
use autostudio_core::constants::CONTEXT_TOOL_RESULT_SPILL_THRESHOLD_BYTES;
use autostudio_core::context::{
    CanonicalMessage, CanonicalToolDefinition, ContextEvent, ContextEventEnvelope,
    ContextEventStore, ContextStoreError, InferenceFinishReason, InferenceItemDraft,
    InferenceTurnId, ProviderBinding, TokenBudgetPlan, VisibleMessageRole,
};
use autostudio_core::context_retrieval::{ContextRetrievalQuery, ContextRetrievalSelection};
use autostudio_core::context_surface::{
    ContextPreparationReason, ContextPressure, ContextSpillBlob, ContextSurfaceTransform,
};
use autostudio_core::project::ProjectService;
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
fn large_tool_result_is_spilled_atomically_and_survives_project_backup() {
    const TAIL_SENTINEL: &str = "FULL_RESULT_TAIL_SENTINEL";

    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("spill.autostudio");
    let backups = temp.path().join("backups");
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("context store"));
    let projects = ProjectService::new(store.clone());
    projects.create_project("Spill contract").expect("project");
    let manager = ContextManager::new(store.clone());
    let run_id = AgentRunId::new();
    let first_turn_id = InferenceTurnId::new();
    let first = manager
        .prepare_turn(prepare(
            run_id.clone(),
            first_turn_id.clone(),
            1,
            "Inspect a large local analysis result",
        ))
        .expect("first Context");
    let recorded = manager
        .record_turn(RecordInferenceTurn {
            run_id: run_id.clone(),
            turn_id: first_turn_id,
            context_id: first.manifest().context_id().clone(),
            expected_journal_revision: first.journal_revision(),
            items: vec![
                InferenceItemDraft::ToolRequest {
                    call_id: "spill-call-1".to_owned(),
                    name: "project_add_midi_notes".to_owned(),
                    arguments_json: r#"{"track":"piano"}"#.to_owned(),
                    descriptor_fingerprint: empty_digest(),
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: None,
                },
            ],
        })
        .expect("Tool Request");
    let large_result = format!(
        "safe-preview|{}|{TAIL_SENTINEL}",
        "x".repeat(CONTEXT_TOOL_RESULT_SPILL_THRESHOLD_BYTES + 1_024)
    );
    manager
        .record_tool_results(RecordToolResults {
            run_id: run_id.clone(),
            expected_journal_revision: recorded.journal_revision,
            results: vec![CompletedToolResult {
                call_id: "spill-call-1".to_owned(),
                name: "project_add_midi_notes".to_owned(),
                content: large_result.clone(),
                is_error: false,
                execution_id: Some("spill-execution-1".to_owned()),
            }],
        })
        .expect("large Tool Result");

    let second = manager
        .prepare_turn(prepare(
            run_id.clone(),
            InferenceTurnId::new(),
            2,
            "Continue from the bounded result",
        ))
        .expect("spilled Context");
    let metrics = second
        .manifest()
        .surface_metrics()
        .expect("surface metrics");
    assert_eq!(metrics.spills().len(), 1);
    assert_eq!(
        metrics.prepared_footprint().pressure(),
        ContextPressure::Normal
    );
    assert!(
        metrics.initial_footprint().message_bytes() > metrics.prepared_footprint().message_bytes()
    );
    let spill_reference = &metrics.spills()[0];
    let model_result = second
        .messages()
        .iter()
        .find_map(|message| match message {
            CanonicalMessage::Tool { content, .. } => Some(content),
            _ => None,
        })
        .expect("model-visible Tool Result");
    assert!(model_result.contains("auto_studio_spilled_tool_result"));
    assert!(model_result.contains(spill_reference.content_hash()));
    assert!(!model_result.contains(TAIL_SENTINEL));

    let stored = store
        .context_spill(spill_reference.content_hash())
        .expect("spill lookup")
        .expect("stored spill");
    assert_eq!(stored.content(), large_result);
    let projection = manager.inspect_run(&run_id).expect("full Transcript");
    assert!(projection.items().iter().any(|item| matches!(
        item.payload(),
        InferenceItemDraft::ToolResult { content, .. } if content.contains(TAIL_SENTINEL)
    )));

    let sink =
        autostudio_storage::ProjectPackageBackup::new(&package, &backups).expect("backup sink");
    let receipt = projects.backup_project(0, &sink).expect("backup Project");
    let receipt = serde_json::to_value(receipt).expect("backup receipt");
    let backup_package = backups.join(receipt["backupName"].as_str().expect("backup name"));
    let backup_store = autostudio_storage::SqliteProjectStore::open(&backup_package)
        .expect("backup context store");
    let backed_up = backup_store
        .context_spill(spill_reference.content_hash())
        .expect("backup spill lookup")
        .expect("backed up spill");
    assert_eq!(backed_up, stored);
}

#[test]
fn an_oversized_first_turn_is_rejected_when_no_safe_compaction_cut_exists() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("pressure.autostudio"))
            .expect("context store"),
    );
    let manager = ContextManager::new(store);
    let run_id = AgentRunId::new();
    let mut request = prepare(
        run_id.clone(),
        InferenceTurnId::new(),
        1,
        &"x".repeat(4_000),
    );
    request.token_budget = TokenBudgetPlan::known(1_000, 100, 50).expect("small known budget");

    let error = manager
        .prepare_turn(request)
        .expect_err("hard pressure must stop before inference");
    assert!(matches!(
        error,
        autostudio_core::context::ContextError::AutomaticCompactionUnavailable
    ));
    let projection = manager.inspect_run(&run_id).expect("empty projection");
    assert_eq!(projection.journal_revision(), 0);
    assert!(projection.items().is_empty());
    assert!(projection.manifests().is_empty());
}

#[test]
fn context_revision_conflict_rolls_back_spill_blob_with_events() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&temp.path().join("atomic-spill.autostudio"))
            .expect("context store"),
    );
    let manager = ContextManager::new(store.clone());
    let run_id = AgentRunId::new();
    let prepared = manager
        .prepare_turn(prepare(
            run_id.clone(),
            InferenceTurnId::new(),
            1,
            "Establish a non-zero Context revision",
        ))
        .expect("prepared Context");
    assert_eq!(prepared.journal_revision(), 2);

    let spill = ContextSpillBlob::new("must roll back with the stale append".to_owned())
        .expect("valid spill blob");
    let error = store
        .append_context_events_with_spills(&run_id, 0, &[], std::slice::from_ref(&spill))
        .expect_err("stale Context append must fail atomically");
    assert!(matches!(
        error,
        ContextStoreError::RevisionConflict {
            expected: 0,
            actual: 2
        }
    ));
    assert!(
        store
            .context_spill(spill.content_hash())
            .expect("spill lookup")
            .is_none(),
        "a rejected Context append must not leave an orphan spill blob"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn automatic_compaction_is_effective_atomic_deterministic_and_restart_safe() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("automatic-compaction.autostudio");
    let run_id = AgentRunId::new();
    let base_store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("context store"));
    let store = Arc::new(FailOnceContextStore::new(base_store.clone()));
    let manager = ContextManager::new(store.clone());

    let first_turn = InferenceTurnId::new();
    let brief = serde_json::json!({
        "summary": "PRIMARY OBJECTIVE",
        "purpose": "soundtrack",
        "targetDurationSeconds": 90,
        "style": ["ambient"],
        "mood": ["hopeful"],
        "instrumentation": ["piano"],
        "constraints": ["no vocals"],
        "padding": "u".repeat(12_000),
    })
    .to_string();
    let first = manager
        .prepare_turn(prepare(run_id.clone(), first_turn.clone(), 1, &brief))
        .expect("large first turn");
    record_visible_turn(
        &manager,
        &run_id,
        first_turn,
        &first,
        &format!("completed first analysis {}", "a".repeat(12_000)),
    );

    for (revision, message) in [
        (2, "second creator decision"),
        (3, "third creator decision"),
    ] {
        let turn_id = InferenceTurnId::new();
        let prepared = manager
            .prepare_turn(prepare(run_id.clone(), turn_id.clone(), revision, message))
            .expect("recent turn");
        record_visible_turn(
            &manager,
            &run_id,
            turn_id,
            &prepared,
            "recent assistant result",
        );
    }

    let before = manager.inspect_run(&run_id).expect("pre-compaction state");
    let before_revision = before.journal_revision();
    let first_kept_after_expected_cut = before.items()[3].id().clone();
    store.fail_next_compaction.store(true, Ordering::SeqCst);
    let recovery_turn = InferenceTurnId::new();
    let pressure_request = || {
        let mut request = prepare(
            run_id.clone(),
            recovery_turn.clone(),
            4,
            "new creator instruction protected from compaction",
        );
        request.token_budget = TokenBudgetPlan::known(6_000, 500, 500).expect("pressure budget");
        request
    };
    let error = manager
        .prepare_turn(pressure_request())
        .expect_err("injected compaction transaction failure");
    assert!(matches!(
        error,
        autostudio_core::context::ContextError::Store(ContextStoreError::Unavailable(_))
    ));
    let after_failure = manager.inspect_run(&run_id).expect("rolled back state");
    assert_eq!(after_failure.journal_revision(), before_revision);
    assert_eq!(after_failure.items(), before.items());
    assert!(after_failure.checkpoints().is_empty());
    let attempted_hash = store
        .attempted_checkpoint_hash
        .lock()
        .expect("attempt hash lock")
        .clone()
        .expect("attempted checkpoint hash");

    let compacted = manager
        .prepare_turn(pressure_request())
        .expect("deterministic compaction retry");
    assert_eq!(
        compacted.manifest().preparation_reason(),
        ContextPreparationReason::PressureCompaction
    );
    let metrics = compacted
        .manifest()
        .surface_metrics()
        .expect("surface metrics");
    assert_eq!(metrics.transform(), ContextSurfaceTransform::Compaction);
    assert_eq!(
        metrics.prepared_footprint().pressure(),
        ContextPressure::Normal
    );
    assert!(
        metrics.prepared_footprint().total_serialized_bytes()
            < metrics.initial_footprint().total_serialized_bytes()
    );
    let projection = manager.inspect_run(&run_id).expect("compacted state");
    assert_eq!(projection.items().len(), before.items().len() + 1);
    assert_eq!(projection.checkpoints().len(), 1);
    let checkpoint = &projection.checkpoints()[0];
    assert_eq!(checkpoint.content_hash(), attempted_hash);
    assert_eq!(checkpoint.summary().objective(), "PRIMARY OBJECTIVE");
    assert!(
        checkpoint
            .summary()
            .constraints()
            .iter()
            .any(|constraint| constraint == "style:ambient")
    );
    assert!(
        checkpoint
            .summary()
            .constraints()
            .iter()
            .any(|constraint| constraint == "constraints:no vocals")
    );
    assert_eq!(checkpoint.replaces_item_ids().len(), 3);
    assert_eq!(
        checkpoint.first_kept_item_id(),
        &first_kept_after_expected_cut
    );
    assert!(matches!(
        compacted.messages().first(),
        Some(CanonicalMessage::ContextSummary { content })
            if content.contains("PRIMARY OBJECTIVE")
    ));
    assert!(matches!(
        compacted.messages().last(),
        Some(CanonicalMessage::User { content })
            if content.contains("protected from compaction")
    ));
    record_visible_turn(
        &manager,
        &run_id,
        recovery_turn,
        &compacted,
        "post-compaction response",
    );
    drop(manager);
    drop(store);
    drop(base_store);

    let reopened_store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("reopened context store"),
    );
    let reopened = ContextManager::new(reopened_store);
    let full_before_next = reopened.inspect_run(&run_id).expect("replayed state");
    assert_eq!(full_before_next.items().len(), before.items().len() + 3);
    let next = reopened
        .prepare_turn(prepare(
            run_id.clone(),
            InferenceTurnId::new(),
            5,
            "continue after process restart",
        ))
        .expect("surface rebuilt from checkpoint");
    assert!(matches!(
        next.messages().first(),
        Some(CanonicalMessage::ContextSummary { content })
            if content.contains("PRIMARY OBJECTIVE")
    ));
    assert!(matches!(
        next.messages().last(),
        Some(CanonicalMessage::User { content }) if content.contains("process restart")
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
        continuity_overhead_tokens: 0,
        tools,
        token_budget: TokenBudgetPlan::known(32_768, 4_096, 1_024).expect("valid token budget"),
    }
}

fn record_visible_turn(
    manager: &ContextManager,
    run_id: &AgentRunId,
    turn_id: InferenceTurnId,
    prepared: &autostudio_core::context::PreparedContext,
    content: &str,
) {
    manager
        .record_turn(RecordInferenceTurn {
            run_id: run_id.clone(),
            turn_id,
            context_id: prepared.manifest().context_id().clone(),
            expected_journal_revision: prepared.journal_revision(),
            items: vec![
                InferenceItemDraft::VisibleMessage {
                    role: VisibleMessageRole::Assistant,
                    content: content.to_owned(),
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: None,
                },
            ],
        })
        .expect("record visible turn");
}

struct FailOnceContextStore {
    inner: Arc<autostudio_storage::SqliteProjectStore>,
    fail_next_compaction: AtomicBool,
    attempted_checkpoint_hash: Mutex<Option<String>>,
}

impl FailOnceContextStore {
    fn new(inner: Arc<autostudio_storage::SqliteProjectStore>) -> Self {
        Self {
            inner,
            fail_next_compaction: AtomicBool::new(false),
            attempted_checkpoint_hash: Mutex::new(None),
        }
    }

    fn reject_compaction_once(&self, events: &[ContextEvent]) -> bool {
        let checkpoint = events.iter().find_map(|event| match event {
            ContextEvent::CompactionCommitted { checkpoint } => Some(checkpoint),
            ContextEvent::InferenceItemAppended { .. } | ContextEvent::ContextPrepared { .. } => {
                None
            }
        });
        let Some(checkpoint) = checkpoint else {
            return false;
        };
        if !self.fail_next_compaction.swap(false, Ordering::SeqCst) {
            return false;
        }
        *self
            .attempted_checkpoint_hash
            .lock()
            .expect("attempt hash lock") = Some(checkpoint.content_hash().to_owned());
        true
    }
}

impl ContextEventStore for FailOnceContextStore {
    fn append_context_events(
        &self,
        run_id: &AgentRunId,
        expected_revision: u64,
        events: &[ContextEvent],
    ) -> Result<u64, ContextStoreError> {
        if self.reject_compaction_once(events) {
            return Err(ContextStoreError::Unavailable(
                "injected pre-commit crash".to_owned(),
            ));
        }
        self.inner
            .append_context_events(run_id, expected_revision, events)
    }

    fn append_context_events_with_spills(
        &self,
        run_id: &AgentRunId,
        expected_revision: u64,
        events: &[ContextEvent],
        spills: &[ContextSpillBlob],
    ) -> Result<u64, ContextStoreError> {
        if self.reject_compaction_once(events) {
            return Err(ContextStoreError::Unavailable(
                "injected pre-commit crash".to_owned(),
            ));
        }
        self.inner
            .append_context_events_with_spills(run_id, expected_revision, events, spills)
    }

    fn context_events(
        &self,
        run_id: &AgentRunId,
    ) -> Result<Vec<ContextEventEnvelope>, ContextStoreError> {
        self.inner.context_events(run_id)
    }

    fn retrieve_context(
        &self,
        query: &ContextRetrievalQuery,
    ) -> Result<Option<ContextRetrievalSelection>, ContextStoreError> {
        self.inner.retrieve_context(query)
    }

    fn context_spill(
        &self,
        content_hash: &str,
    ) -> Result<Option<ContextSpillBlob>, ContextStoreError> {
        self.inner.context_spill(content_hash)
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
