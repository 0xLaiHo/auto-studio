use std::io::Cursor;

use autostudio_music_quality::{ExperimentalMusicSpec, compile_to_smf};
use midly::num::{u4, u7, u28};
use midly::{MetaMessage, MidiMessage, Smf, TrackEvent, TrackEventKind};

use crate::constants::{MIDI_CHANNEL_MAX, MIDI_CHANNEL_MIN, MIDI_MAX};
use crate::error::HandoffError;
use crate::instrument::{InstrumentAssignment, InstrumentAssignmentManifest};

/// Adds portable instrument intent to the frozen Q0 Type-1 MIDI compiler output.
///
/// # Errors
///
/// Returns [`HandoffError`] when the base spec cannot compile, assignments do
/// not match the semantic tracks, or the resulting SMF cannot be encoded.
pub fn compile_portable_smf(
    spec: &ExperimentalMusicSpec,
    manifest: &InstrumentAssignmentManifest,
) -> Result<Vec<u8>, HandoffError> {
    let base = compile_to_smf(spec)?;
    let mut smf = Smf::parse(&base).map_err(|error| HandoffError::MidiParse(error.to_string()))?;
    let expected_tracks = manifest.assignments.len() + 1;
    if smf.tracks.len() != expected_tracks {
        return Err(HandoffError::TrackCount {
            actual: smf.tracks.len(),
            expected: expected_tracks,
        });
    }

    for (track_index, (track, assignment)) in smf
        .tracks
        .iter_mut()
        .skip(1)
        .zip(&manifest.assignments)
        .enumerate()
    {
        let expected_track = &spec.tracks[track_index];
        if assignment.track_id != expected_track.id {
            return Err(HandoffError::AssignmentTrackMismatch {
                assignment_track_id: assignment.track_id.clone(),
                spec_track_id: expected_track.id.clone(),
            });
        }
        validate_assignment(assignment)?;
        if !matches!(
            track.first().map(|event| event.kind),
            Some(TrackEventKind::Meta(MetaMessage::TrackName(_)))
        ) {
            return Err(HandoffError::MissingTrackName {
                track_index: track_index + 1,
            });
        }
        let channel = u4::new(assignment.midi_channel - 1);
        for event in track.iter_mut() {
            if let TrackEventKind::Midi {
                channel: event_channel,
                ..
            } = &mut event.kind
            {
                *event_channel = channel;
            }
        }
        track.splice(1..1, assignment_events(assignment, channel));
    }

    let mut bytes = Vec::new();
    smf.write_std(&mut Cursor::new(&mut bytes))
        .map_err(|error| HandoffError::MidiEncode(error.to_string()))?;
    Ok(bytes)
}

fn validate_assignment(assignment: &InstrumentAssignment) -> Result<(), HandoffError> {
    if !(MIDI_CHANNEL_MIN..=MIDI_CHANNEL_MAX).contains(&assignment.midi_channel) {
        return Err(HandoffError::InvalidAssignment {
            track_id: assignment.track_id.clone(),
            reason: format!("MIDI channel {} is outside 1..=16", assignment.midi_channel),
        });
    }
    if assignment.bank_msb > MIDI_MAX
        || assignment.bank_lsb > MIDI_MAX
        || assignment.program > MIDI_MAX
    {
        return Err(HandoffError::InvalidAssignment {
            track_id: assignment.track_id.clone(),
            reason: format!("Bank/Program values must be inside 0..={MIDI_MAX}"),
        });
    }
    Ok(())
}

fn assignment_events(assignment: &InstrumentAssignment, channel: u4) -> [TrackEvent<'static>; 3] {
    [
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel,
                message: MidiMessage::Controller {
                    controller: u7::new(0),
                    value: u7::new(assignment.bank_msb),
                },
            },
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel,
                message: MidiMessage::Controller {
                    controller: u7::new(32),
                    value: u7::new(assignment.bank_lsb),
                },
            },
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel,
                message: MidiMessage::ProgramChange {
                    program: u7::new(assignment.program),
                },
            },
        },
    ]
}
