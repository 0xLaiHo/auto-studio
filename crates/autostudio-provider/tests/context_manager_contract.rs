use std::sync::Arc;

use autostudio_core::agent::{AgentRunId, InferenceUsage};
use autostudio_core::context::{
    CanonicalMessage, CanonicalToolDefinition, ContextEventStore, ContextStoreError,
    InferenceFinishReason, InferenceItemDraft, InferenceTurnId, ProviderBinding, TokenBudgetPlan,
    VisibleMessageRole,
};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::context::{
    CompletedToolResult, ContextManager, PrepareContext, RecordInferenceTurn, RecordToolResults,
    fingerprint_tool_catalog,
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
