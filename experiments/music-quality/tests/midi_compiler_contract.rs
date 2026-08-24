use autostudio_music_quality::{ExperimentalMusicSpec, compile_to_smf};
use midly::{Format, MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

#[test]
fn compiles_tempo_markers_cc_and_notes_to_parseable_smf() {
    let spec = ExperimentalMusicSpec::parse_and_validate(fixture()).expect("valid fixture");
    let bytes = compile_to_smf(&spec).expect("compiled MIDI");
    let smf = Smf::parse(&bytes).expect("parse generated MIDI");

    assert_eq!(smf.header.format, Format::Parallel);
    assert_eq!(smf.header.timing, Timing::Metrical(480.into()));
    assert_eq!(smf.tracks.len(), 2, "conductor plus one musical track");

    assert!(smf.tracks[0].iter().any(|event| matches!(
        event.kind,
        TrackEventKind::Meta(MetaMessage::Tempo(value)) if value.as_int() == 500_000
    )));
    assert!(smf.tracks[0].iter().any(|event| matches!(
        event.kind,
        TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8))
    )));
    assert!(smf.tracks[0].iter().any(|event| matches!(
        event.kind,
        TrackEventKind::Meta(MetaMessage::Marker(name)) if name == b"Intro"
    )));

    let absolute = absolute_midi_events(&smf.tracks[1]);
    assert!(absolute.iter().any(|(tick, event)| {
        *tick == 0
            && matches!(
                event,
                MidiMessage::Controller { controller, value }
                    if controller.as_int() == 1 && value.as_int() == 32
            )
    }));
    assert!(absolute.iter().any(|(tick, event)| {
        *tick == 0
            && matches!(
                event,
                MidiMessage::NoteOn { key, vel }
                    if key.as_int() == 60 && vel.as_int() == 96
            )
    }));
    assert!(absolute.iter().any(|(tick, event)| {
        *tick == 480
            && matches!(
                event,
                MidiMessage::NoteOff { key, vel }
                    if key.as_int() == 60 && vel.as_int() == 0
            )
    }));
}

fn absolute_midi_events(track: &[midly::TrackEvent<'_>]) -> Vec<(u64, MidiMessage)> {
    let mut tick = 0_u64;
    track
        .iter()
        .filter_map(|event| {
            tick += u64::from(event.delta.as_int());
            match event.kind {
                TrackEventKind::Midi { message, .. } => Some((tick, message)),
                _ => None,
            }
        })
        .collect()
}

fn fixture() -> &'static str {
    r#"{
      "title": "MIDI contract",
      "tempo_map": [
        {"bar": 1, "bpm": 120.0, "time_signature": {"numerator": 4, "denominator": 4}}
      ],
      "key_map": [{"bar": 1, "tonic": "C", "mode": "major"}],
      "sections": [
        {"id": "intro", "label": "Intro", "start_bar": 1, "length_bars": 2, "intent": "state motif"}
      ],
      "tracks": [{
        "id": "lead", "name": "Lead", "role": "melody",
        "register": {"low": 48, "high": 84}, "instrument_hint": "electric piano",
        "regions": [{
          "section_id": "intro",
          "notes": [{"beat": 0.0, "duration": 1.0, "pitch": 60, "velocity": 96}],
          "cc": [{"beat": 0.0, "controller": 1, "value": 32}]
        }]
      }]
    }"#
}
