use autostudio_music_quality::ExperimentalMusicSpec;
use autostudio_portable_handoff::{compile_portable_smf, resolve_instrument_assignments};
use midly::{Format, MidiMessage, Smf, Timing, TrackEventKind};

#[test]
fn writes_bank_program_and_rewrites_each_track_to_its_assigned_channel() {
    let spec = ExperimentalMusicSpec::parse_and_validate(pilot_fixture()).expect("valid Pilot");
    let assignments = resolve_instrument_assignments(&spec).expect("assignments");

    let bytes = compile_portable_smf(&spec, &assignments).expect("portable MIDI");
    let smf = Smf::parse(&bytes).expect("parse generated MIDI");

    assert_eq!(smf.header.format, Format::Parallel);
    assert_eq!(smf.header.timing, Timing::Metrical(480.into()));
    assert_eq!(smf.tracks.len(), 4, "conductor plus three music tracks");

    for (track, assignment) in smf.tracks.iter().skip(1).zip(&assignments.assignments) {
        let events = absolute_midi_events(track);
        let expected_channel = assignment.midi_channel - 1;
        assert!(
            events
                .iter()
                .all(|(_, channel, _)| *channel == expected_channel)
        );
        assert!(events.iter().any(|(tick, _, message)| {
            *tick == 0
                && matches!(
                    message,
                    MidiMessage::Controller { controller, value }
                        if controller.as_int() == 0 && value.as_int() == assignment.bank_msb
                )
        }));
        assert!(events.iter().any(|(tick, _, message)| {
            *tick == 0
                && matches!(
                    message,
                    MidiMessage::Controller { controller, value }
                        if controller.as_int() == 32 && value.as_int() == assignment.bank_lsb
                )
        }));
        assert!(events.iter().any(|(tick, _, message)| {
            *tick == 0
                && matches!(
                    message,
                    MidiMessage::ProgramChange { program }
                        if program.as_int() == assignment.program
                )
        }));
    }
}

#[test]
fn rejects_an_invalid_assignment_before_constructing_midi_values() {
    let spec = ExperimentalMusicSpec::parse_and_validate(pilot_fixture()).expect("valid Pilot");
    let mut assignments = resolve_instrument_assignments(&spec).expect("assignments");
    assignments.assignments[0].midi_channel = 0;

    let error = compile_portable_smf(&spec, &assignments).expect_err("invalid channel");

    assert!(error.to_string().contains("outside 1..=16"));
}

fn absolute_midi_events(track: &[midly::TrackEvent<'_>]) -> Vec<(u64, u8, MidiMessage)> {
    let mut tick = 0_u64;
    track
        .iter()
        .filter_map(|event| {
            tick += u64::from(event.delta.as_int());
            match event.kind {
                TrackEventKind::Midi { channel, message } => {
                    Some((tick, channel.as_int(), message))
                }
                _ => None,
            }
        })
        .collect()
}

fn pilot_fixture() -> &'static str {
    include_str!("../../music-quality/evidence/pilot/l1-song-hook/spec.json")
}
