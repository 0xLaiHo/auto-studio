use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use autostudio_core::agent::AgentRunId;
use autostudio_core::compaction::{StructuredRunSummary, StructuredRunSummaryDraft};
use autostudio_core::constants::{
    CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS, CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
};
use autostudio_core::context::{
    CanonicalMessage, CanonicalToolDefinition, InferenceFinishReason, InferenceItemDraft,
    InferenceTurnId, ProviderBinding, TokenBudgetPlan, VisibleMessageRole,
};
use autostudio_core::context_retrieval::{
    ContextRetrievalQuery, ContextRetrievalReason, ContextRetrievalSourceType,
};
use autostudio_core::provider::{ThinkingControl, ThinkingLevel};
use autostudio_provider::context::{
    CommitCompaction, CompletedToolResult, ContextManager, PrepareContext, RecordInferenceTurn,
    RecordToolResults, fingerprint_tool_catalog,
};
use sha2::{Digest, Sha256};

#[test]
#[allow(clippy::too_many_lines)]
fn bm25_retrieval_is_source_linked_manifested_untrusted_and_rebuildable() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("retrieval.autostudio");
    let run_id = AgentRunId::new();
    let store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("Context store"));
    let manager = ContextManager::new(store.clone());

    let first_turn = InferenceTurnId::new();
    let first = manager
        .prepare_turn(prepare(
            run_id.clone(),
            first_turn.clone(),
            7,
            "Keep the arrangement instrumental. The vocal constraint codename is ultramarine.",
        ))
        .expect("first turn");
    let first_source = manager
        .inspect_run(&run_id)
        .expect("first projection")
        .items()[0]
        .clone();
    let recorded = manager
        .record_turn(RecordInferenceTurn {
            run_id: run_id.clone(),
            turn_id: first_turn,
            context_id: first.manifest().context_id().clone(),
            expected_journal_revision: first.journal_revision(),
            items: vec![
                InferenceItemDraft::VisibleMessage {
                    role: VisibleMessageRole::Assistant,
                    content: "Creator decided to preserve the no-vocals constraint.".to_owned(),
                },
                InferenceItemDraft::ToolRequest {
                    call_id: "artifact-call".to_owned(),
                    name: "project_describe".to_owned(),
                    arguments_json: "{}".to_owned(),
                    descriptor_fingerprint: empty_digest(),
                },
                InferenceItemDraft::ToolRequest {
                    call_id: "unresolved-call".to_owned(),
                    name: "project_describe".to_owned(),
                    arguments_json: "{}".to_owned(),
                    descriptor_fingerprint: empty_digest(),
                },
                InferenceItemDraft::Finish {
                    reason: InferenceFinishReason::Completed,
                    detail: None,
                },
            ],
        })
        .expect("record first turn");
    manager
        .record_tool_results(RecordToolResults {
            run_id: run_id.clone(),
            expected_journal_revision: recorded.journal_revision,
            results: vec![
                CompletedToolResult {
                    call_id: "artifact-call".to_owned(),
                    name: "project_describe".to_owned(),
                    content: r#"{"artifact":"mixdown_cerulean.wav"}"#.to_owned(),
                    is_error: false,
                    execution_id: Some("execution-artifact-1".to_owned()),
                },
                CompletedToolResult {
                    call_id: "unresolved-call".to_owned(),
                    name: "project_describe".to_owned(),
                    content: "missing sampler preset remains unresolved".to_owned(),
                    is_error: true,
                    execution_id: Some("execution-error-1".to_owned()),
                },
            ],
        })
        .expect("record Tool Results");

    let second_turn = InferenceTurnId::new();
    let second = manager
        .prepare_turn(prepare(
            run_id.clone(),
            second_turn.clone(),
            8,
            "Add a restrained piano pulse.",
        ))
        .expect("second turn");
    record_visible_turn(
        &manager,
        &run_id,
        second_turn,
        &second,
        "The piano pulse is ready.",
    );

    let before_compaction = manager.inspect_run(&run_id).expect("full Transcript");
    let first_kept = before_compaction.items()[7].id().clone();
    manager
        .commit_compaction(CommitCompaction {
            run_id: run_id.clone(),
            expected_journal_revision: before_compaction.journal_revision(),
            replaces_item_ids: before_compaction.items()[..7]
                .iter()
                .map(|item| item.id().clone())
                .collect(),
            first_kept_item_id: first_kept,
            summary: summary(),
        })
        .expect("compaction");

    let exact_query = ContextRetrievalQuery::new(
        run_id.clone(),
        None,
        vec![first_source.id().clone()],
        Vec::new(),
        ContextRetrievalReason::ExactSourceReference,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
    )
    .expect("exact query");
    let exact = manager
        .retrieve_context(&exact_query)
        .expect("exact retrieval")
        .expect("exact hit");
    assert_eq!(exact.hits()[0].item_id(), first_source.id());

    let decision_query = ContextRetrievalQuery::new(
        run_id.clone(),
        Some("Creator decided preserve no-vocals".to_owned()),
        Vec::new(),
        Vec::new(),
        ContextRetrievalReason::CurrentInputSimilarity,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
    )
    .expect("Creator decision query")
    .with_source_types(vec![ContextRetrievalSourceType::AssistantMessage])
    .expect("Assistant-only filter");
    let decision = manager
        .retrieve_context(&decision_query)
        .expect("Creator decision retrieval")
        .expect("Creator decision hit");
    assert!(decision.hits().iter().any(|hit| {
        hit.source_type() == ContextRetrievalSourceType::AssistantMessage
            && hit.excerpt().contains("Creator decided")
    }));

    let artifact_query = ContextRetrievalQuery::new(
        run_id.clone(),
        Some("mixdown_cerulean artifact".to_owned()),
        Vec::new(),
        Vec::new(),
        ContextRetrievalReason::CurrentInputSimilarity,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
    )
    .expect("artifact query")
    .with_source_types(vec![ContextRetrievalSourceType::ToolResult])
    .expect("Tool Result filter");
    let artifact = manager
        .retrieve_context(&artifact_query)
        .expect("artifact retrieval")
        .expect("artifact hit");
    assert!(artifact.hits().iter().any(|hit| {
        hit.source_type() == ContextRetrievalSourceType::ToolResult
            && hit.execution_id() == Some("execution-artifact-1")
            && !hit.is_error()
            && hit.excerpt().contains("mixdown_cerulean.wav")
    }));
    let unresolved_query = ContextRetrievalQuery::new(
        run_id.clone(),
        Some("missing sampler preset unresolved".to_owned()),
        Vec::new(),
        Vec::new(),
        ContextRetrievalReason::CurrentInputSimilarity,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_HITS,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
    )
    .expect("unresolved query")
    .with_source_types(vec![ContextRetrievalSourceType::ToolResult])
    .expect("Tool Result filter");
    let unresolved = manager
        .retrieve_context(&unresolved_query)
        .expect("unresolved retrieval")
        .expect("unresolved hit");
    assert!(unresolved.hits().iter().any(|hit| {
        hit.source_type() == ContextRetrievalSourceType::ToolResult
            && hit.execution_id() == Some("execution-error-1")
            && hit.is_error()
            && hit.excerpt().contains("remains unresolved")
    }));

    let recalled = manager
        .prepare_turn(prepare(
            run_id.clone(),
            InferenceTurnId::new(),
            9,
            "Check the ultramarine vocal constraint before continuing.",
        ))
        .expect("retrieval-aware turn");
    let selection = recalled
        .manifest()
        .retrieval_selection()
        .expect("audited selection");
    assert_eq!(
        selection.reason(),
        ContextRetrievalReason::CurrentInputSimilarity
    );
    assert!(selection.estimated_tokens() > 0);
    let hit = selection
        .hits()
        .iter()
        .find(|hit| hit.item_id() == first_source.id())
        .expect("early Creator constraint recalled");
    assert_eq!(
        hit.source_type(),
        ContextRetrievalSourceType::CreatorMessage
    );
    assert_eq!(hit.project_revision(), 7);
    assert_eq!(hit.content_hash(), first_source.content_hash());
    assert!(hit.excerpt().contains("ultramarine"));
    assert!(recalled.messages().iter().any(|message| matches!(
        message,
        CanonicalMessage::RetrievedContext { content }
            if content.contains("UNTRUSTED RETRIEVED CONTEXT")
                && content.contains("ultramarine")
    )));
    assert!(selection.hits().iter().all(|hit| {
        !recalled
            .manifest()
            .included_item_ids()
            .contains(hit.item_id())
    }));

    drop(manager);
    drop(store);
    let database = rusqlite::Connection::open(package.join("project.db")).expect("raw database");
    database
        .execute("DELETE FROM inference_context_retrieval", [])
        .expect("delete rebuildable projection");
    drop(database);

    let reopened_store = Arc::new(
        autostudio_storage::SqliteProjectStore::open(&package).expect("reopened Context store"),
    );
    let reopened = ContextManager::new(reopened_store);
    let rebuilt = reopened
        .retrieve_context(&exact_query)
        .expect("rebuilt exact retrieval")
        .expect("rebuilt hit");
    assert_eq!(rebuilt.hits()[0].item_id(), first_source.id());
    assert_eq!(
        rebuilt.hits()[0].content_hash(),
        first_source.content_hash()
    );
    assert_eq!(rebuilt, exact);
}

#[test]
#[allow(clippy::too_many_lines)]
fn frozen_long_run_survives_one_hundred_steps_ten_compactions_and_three_restarts() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let package = temp.path().join("long-run.autostudio");
    let run_id = AgentRunId::new();
    let mut store =
        Arc::new(autostudio_storage::SqliteProjectStore::open(&package).expect("Context store"));
    let mut manager = ContextManager::new(store.clone());
    let mut first_source = None;
    let mut recalled_anchor = false;
    let mut restart_count = 0_u8;

    for step in 1_u64..=100 {
        let turn_id = InferenceTurnId::new();
        let message = if step == 1 {
            "ANCHOR_CERULEAN: preserve the no-vocals constraint and artifact lineage.".to_owned()
        } else if step == 100 {
            "Before the final pass, recall ANCHOR_CERULEAN and its no-vocals constraint.".to_owned()
        } else {
            format!("iterationtoken{step:03}: continue the arrangement")
        };
        let prepared = manager
            .prepare_turn(prepare(run_id.clone(), turn_id.clone(), step, &message))
            .expect("long-Run turn preparation");
        if step == 1 {
            first_source = manager
                .inspect_run(&run_id)
                .expect("first projection")
                .items()
                .first()
                .cloned();
        }
        if step == 100 {
            let first_source = first_source.as_ref().expect("first source");
            recalled_anchor = prepared
                .manifest()
                .retrieval_selection()
                .is_some_and(|selection| {
                    selection
                        .hits()
                        .iter()
                        .any(|hit| hit.item_id() == first_source.id())
                });
        }
        record_visible_turn(
            &manager,
            &run_id,
            turn_id,
            &prepared,
            &format!("completed iteration {step:03}"),
        );

        if step % 10 == 0 {
            let projection = manager.inspect_run(&run_id).expect("long-Run projection");
            let first_kept_index = usize::try_from((step - 2) * 3).expect("kept index");
            assert_eq!(projection.items().len(), usize::try_from(step * 3).unwrap());
            manager
                .commit_compaction(CommitCompaction {
                    run_id: run_id.clone(),
                    expected_journal_revision: projection.journal_revision(),
                    replaces_item_ids: projection.items()[..first_kept_index]
                        .iter()
                        .map(|item| item.id().clone())
                        .collect(),
                    first_kept_item_id: projection.items()[first_kept_index].id().clone(),
                    summary: StructuredRunSummary::new(StructuredRunSummaryDraft {
                        objective: "Preserve the durable creative objective across the long Run."
                            .to_owned(),
                        creator_decisions: vec![format!("compacted through step {step:03}")],
                        constraints: vec!["honor source-linked constraints".to_owned()],
                        completed_work: Vec::new(),
                        open_items: Vec::new(),
                        artifact_references: Vec::new(),
                    })
                    .expect("long-Run summary"),
                })
                .expect("long-Run compaction");
        }

        if matches!(step, 25 | 50 | 75) {
            drop(manager);
            drop(store);
            if step == 25 {
                let old_timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time")
                    .saturating_sub(Duration::from_hours(48))
                    .as_millis();
                let old_timestamp = u64::try_from(old_timestamp).expect("timestamp");
                let database =
                    rusqlite::Connection::open(package.join("project.db")).expect("raw database");
                database
                    .execute(
                        "UPDATE inference_context_events
                         SET event_json = json_set(event_json, '$.item.createdAtUnixMillis', ?1)
                         WHERE run_id = ?2 AND sequence = 1",
                        rusqlite::params![
                            i64::try_from(old_timestamp).expect("SQLite timestamp"),
                            run_id.as_str()
                        ],
                    )
                    .expect("simulate a cross-day Run");
            }
            store = Arc::new(
                autostudio_storage::SqliteProjectStore::open(&package)
                    .expect("reopened Context store"),
            );
            manager = ContextManager::new(store.clone());
            restart_count += 1;
        }
    }

    let final_projection = manager.inspect_run(&run_id).expect("final projection");
    assert_eq!(final_projection.items().len(), 300);
    assert_eq!(final_projection.checkpoints().len(), 10);
    assert_eq!(restart_count, 3);
    assert!(
        recalled_anchor,
        "the compacted first-turn constraint must be recalled"
    );
    let first_source = first_source.expect("first source");
    let exact_query = ContextRetrievalQuery::new(
        run_id,
        None,
        vec![first_source.id().clone()],
        Vec::new(),
        ContextRetrievalReason::ExactSourceReference,
        1,
        CONTEXT_RETRIEVAL_DEFAULT_MAX_TOKENS,
    )
    .expect("exact query");
    let final_hit = manager
        .retrieve_context(&exact_query)
        .expect("final exact retrieval")
        .expect("final hit");
    assert_eq!(
        final_hit.hits()[0].content_hash(),
        first_source.content_hash()
    );
    let one_day_millis = 24 * 60 * 60 * 1_000;
    let now = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_millis(),
    )
    .expect("timestamp");
    assert!(
        now.saturating_sub(final_hit.hits()[0].created_at_unix_millis()) >= one_day_millis,
        "the Run must remain retrievable after a simulated cross-day recovery"
    );
}

fn prepare(
    run_id: AgentRunId,
    turn_id: InferenceTurnId,
    project_revision: u64,
    message: &str,
) -> PrepareContext {
    let tools = vec![
        CanonicalToolDefinition::new(
            "project_describe",
            "Read Project facts",
            r#"{"type":"object","additionalProperties":false}"#,
            empty_digest(),
        )
        .expect("Tool definition"),
    ];
    PrepareContext {
        run_id,
        turn_id,
        project_id: "retrieval-contract-project".to_owned(),
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

fn summary() -> StructuredRunSummary {
    StructuredRunSummary::new(StructuredRunSummaryDraft {
        objective: "Develop the cue while preserving earlier source-linked decisions.".to_owned(),
        creator_decisions: Vec::new(),
        constraints: Vec::new(),
        completed_work: Vec::new(),
        open_items: Vec::new(),
        artifact_references: Vec::new(),
    })
    .expect("summary")
}

fn empty_digest() -> String {
    format!("sha256:{:x}", Sha256::digest([]))
}
