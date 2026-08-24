use autostudio_music_quality::ExperimentalMusicSpec;

#[test]
fn parses_a_valid_multisection_music_spec() {
    let spec = ExperimentalMusicSpec::parse_and_validate(valid_spec()).expect("valid spec");

    assert_eq!(spec.sections.len(), 2);
    assert_eq!(spec.tracks.len(), 2);
    assert_eq!(spec.tracks[0].regions[0].notes[0].pitch, 60);
}

#[test]
fn rejects_duplicate_ids_and_overlapping_sections() {
    let mut value: serde_json::Value = serde_json::from_str(valid_spec()).expect("fixture JSON");
    value["sections"][1]["id"] = serde_json::json!("intro");
    value["sections"][1]["start_bar"] = serde_json::json!(4);

    let error = ExperimentalMusicSpec::parse_and_validate(&value.to_string())
        .expect_err("invalid identity and timeline");

    assert!(error.to_string().contains("duplicate section id `intro`"));
    assert!(error.to_string().contains("overlaps"));
}

#[test]
fn rejects_unknown_sections_and_notes_outside_region_or_register() {
    let mut value: serde_json::Value = serde_json::from_str(valid_spec()).expect("fixture JSON");
    value["tracks"][0]["regions"][0]["section_id"] = serde_json::json!("missing");
    value["tracks"][0]["regions"][0]["notes"][0]["beat"] = serde_json::json!(100.0);
    value["tracks"][0]["regions"][0]["notes"][0]["pitch"] = serde_json::json!(20);

    let error = ExperimentalMusicSpec::parse_and_validate(&value.to_string())
        .expect_err("invalid region references");

    let message = error.to_string();
    assert!(message.contains("unknown section `missing`"));
    assert!(message.contains("outside section duration"));
    assert!(message.contains("outside track register 48..=84"));
}

#[test]
fn rejects_time_signature_numerator_above_schema_limit() {
    let mut value: serde_json::Value = serde_json::from_str(valid_spec()).expect("fixture JSON");
    value["tempo_map"][0]["time_signature"]["numerator"] = serde_json::json!(33);

    let error = ExperimentalMusicSpec::parse_and_validate(&value.to_string())
        .expect_err("numerator above the frozen schema maximum");

    assert!(
        error
            .to_string()
            .contains("unsupported time signature 33/4")
    );
}

#[test]
fn rejects_a_spec_above_the_global_note_and_cc_budgets() {
    let mut value: serde_json::Value = serde_json::from_str(valid_spec()).expect("fixture JSON");
    let note = value["tracks"][0]["regions"][0]["notes"][0].clone();
    let cc = value["tracks"][0]["regions"][0]["cc"][0].clone();
    value["tracks"][0]["regions"][0]["notes"] = serde_json::Value::Array(vec![note; 769]);
    value["tracks"][0]["regions"][0]["cc"] = serde_json::Value::Array(vec![cc; 257]);

    let error = ExperimentalMusicSpec::parse_and_validate(&value.to_string())
        .expect_err("global event budgets");

    let message = error.to_string();
    assert!(message.contains("total notes 770 exceeds 768"));
    assert!(message.contains("total CC events 257 exceeds 256"));
}

fn valid_spec() -> &'static str {
    r#"{
      "title": "Contract fixture",
      "tempo_map": [
        {"bar": 1, "bpm": 120.0, "time_signature": {"numerator": 4, "denominator": 4}}
      ],
      "key_map": [{"bar": 1, "tonic": "C", "mode": "major"}],
      "sections": [
        {"id": "intro", "label": "Intro", "start_bar": 1, "length_bars": 4, "intent": "state motif"},
        {"id": "verse", "label": "Verse", "start_bar": 5, "length_bars": 4, "intent": "develop motif"}
      ],
      "tracks": [
        {
          "id": "lead", "name": "Lead", "role": "melody",
          "register": {"low": 48, "high": 84}, "instrument_hint": "electric piano",
          "regions": [{
            "section_id": "intro",
            "notes": [{"beat": 0.0, "duration": 1.0, "pitch": 60, "velocity": 96}],
            "cc": [{"beat": 0.0, "controller": 1, "value": 32}]
          }]
        },
        {
          "id": "bass", "name": "Bass", "role": "bass",
          "register": {"low": 28, "high": 55}, "instrument_hint": "electric bass",
          "regions": [{
            "section_id": "verse",
            "notes": [{"beat": 0.0, "duration": 2.0, "pitch": 36, "velocity": 88}],
            "cc": []
          }]
        }
      ]
    }"#
}
