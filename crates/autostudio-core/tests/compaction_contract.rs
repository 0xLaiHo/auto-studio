use autostudio_core::agent::AgentRunId;
use autostudio_core::compaction::{
    CompactionCheckpoint, CompactionError, StructuredRunSummary, StructuredRunSummaryDraft,
};
use autostudio_core::context::InferenceItemId;

#[test]
fn identical_source_facts_have_a_stable_hash_without_reusing_checkpoint_identity() {
    let run_id = AgentRunId::new();
    let replaced = vec![InferenceItemId::new(), InferenceItemId::new()];
    let first_kept = InferenceItemId::new();
    let summary = summary();

    let first = CompactionCheckpoint::new(
        run_id.clone(),
        12,
        replaced.clone(),
        first_kept.clone(),
        summary.clone(),
        1_725_000_000_000,
    )
    .expect("first checkpoint");
    let second =
        CompactionCheckpoint::new(run_id, 12, replaced, first_kept, summary, 1_725_000_999_999)
            .expect("second checkpoint");

    assert_ne!(first.id(), second.id());
    assert_ne!(
        first.created_at_unix_millis(),
        second.created_at_unix_millis()
    );
    assert_eq!(first.content_hash(), second.content_hash());
    first.validate().expect("valid first checkpoint");
    second.validate().expect("valid second checkpoint");
}

#[test]
fn checkpoint_rejects_duplicate_replacements_and_detects_hash_tampering() {
    let item = InferenceItemId::new();
    let error = CompactionCheckpoint::new(
        AgentRunId::new(),
        4,
        vec![item.clone(), item],
        InferenceItemId::new(),
        summary(),
        1,
    )
    .expect_err("duplicate replacements must fail");
    assert_eq!(error, CompactionError::InvalidReplacementSet);

    let checkpoint = CompactionCheckpoint::new(
        AgentRunId::new(),
        4,
        vec![InferenceItemId::new()],
        InferenceItemId::new(),
        summary(),
        1,
    )
    .expect("checkpoint");
    let mut json = serde_json::to_value(checkpoint).expect("checkpoint JSON");
    json["contentHash"] = serde_json::Value::String(format!("sha256:{}", "0".repeat(64)));
    let tampered: CompactionCheckpoint =
        serde_json::from_value(json).expect("shape remains deserializable");
    assert_eq!(
        tampered.validate(),
        Err(CompactionError::ContentHashMismatch)
    );
}

fn summary() -> StructuredRunSummary {
    StructuredRunSummary::new(StructuredRunSummaryDraft {
        objective: "Create an editable eight-bar cue".to_owned(),
        creator_decisions: vec!["Keep the C minor harmony".to_owned()],
        constraints: vec!["No vocals".to_owned()],
        completed_work: vec!["Inspected the Project".to_owned()],
        open_items: vec!["Revise the ending".to_owned()],
        artifact_references: vec!["artifact:preview-1".to_owned()],
    })
    .expect("structured summary")
}
