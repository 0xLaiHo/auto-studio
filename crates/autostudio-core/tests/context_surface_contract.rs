use autostudio_core::constants::CONTEXT_ESTIMATED_BYTES_PER_TOKEN;
use autostudio_core::context::{CanonicalMessage, TokenBudgetPlan};
use autostudio_core::context_surface::{
    ContextFootprint, ContextPressure, ContextSpillBlob, ContextSurfaceError,
    ContextSurfaceMetrics, ContextSurfaceTransform,
};

#[test]
fn canonical_request_footprint_classifies_known_and_unknown_pressure_deterministically() {
    let messages = vec![CanonicalMessage::User {
        content: "x".repeat(900),
    }];
    let unknown = ContextFootprint::measure(
        "system",
        &messages,
        &[],
        17,
        &TokenBudgetPlan::unknown(100, 50),
    )
    .expect("unknown footprint");
    assert_eq!(unknown.pressure(), ContextPressure::Unknown);
    assert_eq!(unknown.continuity_overhead_tokens(), 17);
    assert_eq!(
        unknown.estimated_input_tokens(),
        unknown
            .total_serialized_bytes()
            .div_ceil(CONTEXT_ESTIMATED_BYTES_PER_TOKEN)
            + 17
    );

    let budget = unknown.estimated_input_tokens() + 1;
    let known = ContextFootprint::measure(
        "system",
        &messages,
        &[],
        17,
        &TokenBudgetPlan::known(budget + 150, 100, 50).expect("known budget"),
    )
    .expect("known footprint");
    assert_eq!(known.pressure(), ContextPressure::Hard);

    let soft_input_budget = unknown.estimated_input_tokens() * 5 / 4;
    let soft = ContextFootprint::measure(
        "system",
        &messages,
        &[],
        17,
        &TokenBudgetPlan::known(soft_input_budget + 150, 100, 50).expect("soft budget"),
    )
    .expect("soft footprint");
    assert_eq!(soft.pressure(), ContextPressure::Soft);

    let normal_input_budget = unknown.estimated_input_tokens() * 2;
    let normal = ContextFootprint::measure(
        "system",
        &messages,
        &[],
        17,
        &TokenBudgetPlan::known(normal_input_budget + 150, 100, 50).expect("normal budget"),
    )
    .expect("normal footprint");
    assert_eq!(normal.pressure(), ContextPressure::Normal);

    let overflow_budget = unknown.estimated_input_tokens() - 1;
    let overflow = ContextFootprint::measure(
        "system",
        &messages,
        &[],
        17,
        &TokenBudgetPlan::known(overflow_budget + 150, 100, 50).expect("overflow budget"),
    )
    .expect("overflow footprint");
    assert_eq!(overflow.pressure(), ContextPressure::Overflow);

    let mut tampered = serde_json::to_value(overflow).expect("footprint JSON");
    tampered["estimatedInputTokens"] = serde_json::Value::from(1);
    let tampered: ContextFootprint =
        serde_json::from_value(tampered).expect("tampered footprint shape");
    assert_eq!(
        tampered.validate(),
        Err(ContextSurfaceError::InvalidFootprint)
    );
}

#[test]
fn spill_blob_is_content_addressed_and_detects_tampering() {
    let first = ContextSpillBlob::new("large deterministic Tool Result".to_owned()).expect("blob");
    let second = ContextSpillBlob::new("large deterministic Tool Result".to_owned()).expect("blob");
    assert_eq!(first.content_hash(), second.content_hash());
    assert_eq!(first.byte_count(), 31);

    let mut json = serde_json::to_value(first).expect("blob JSON");
    json["content"] = serde_json::Value::String("tampered".to_owned());
    let tampered: ContextSpillBlob = serde_json::from_value(json).expect("tampered shape");
    assert_eq!(
        tampered.validate(),
        Err(ContextSurfaceError::SpillHashMismatch)
    );
}

#[test]
fn legacy_surface_metrics_without_a_transform_remain_replayable() {
    let budget = TokenBudgetPlan::known(4_096, 512, 256).expect("budget");
    let footprint = ContextFootprint::measure(
        "system",
        &[CanonicalMessage::User {
            content: "legacy context".to_owned(),
        }],
        &[],
        0,
        &budget,
    )
    .expect("footprint");
    let metrics = ContextSurfaceMetrics::new(footprint.clone(), footprint, Vec::new(), false);
    let mut json = serde_json::to_value(metrics).expect("metrics JSON");
    json.as_object_mut()
        .expect("metrics object")
        .remove("transform");
    json["formatRevision"] = serde_json::Value::String("autostudio.context-surface/1".to_owned());
    let legacy: ContextSurfaceMetrics = serde_json::from_value(json).expect("legacy metrics shape");

    assert_eq!(legacy.transform(), ContextSurfaceTransform::None);
    legacy.validate().expect("legacy metrics remain valid");
}
