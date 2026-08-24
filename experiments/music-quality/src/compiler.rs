use midly::num::{u4, u7, u15, u24, u28};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

use crate::constants::{MIDI_MAX_DELTA, MIDI_TICKS_PER_QUARTER};
use crate::error::CompileError;
use crate::spec::{ExperimentalMusicSpec, TimeSignature};

type TimedEvent<'a> = (u64, u8, TrackEventKind<'a>);

/// Compiles a validated experimental music specification to type-1 SMF MIDI.
///
/// Track zero carries tempo, time signature, key signature and section markers.
/// Every semantic music track becomes one independent MIDI track.
///
/// # Errors
///
/// Returns [`CompileError`] when the specification is invalid, the timeline is
/// too large for SMF delta times, or the encoder cannot write the result.
pub fn compile_to_smf(spec: &ExperimentalMusicSpec) -> Result<Vec<u8>, CompileError> {
    let violations = spec.violations();
    if !violations.is_empty() {
        return Err(CompileError::InvalidSpec(violations.join("; ")));
    }

    let mut tracks = Vec::with_capacity(spec.tracks.len() + 1);
    tracks.push(conductor_track(spec)?);
    for (index, track) in spec.tracks.iter().enumerate() {
        let channel =
            u4::new(u8::try_from(index % 16).map_err(|_| CompileError::TimelineOverflow)?);
        let mut events = vec![(
            0,
            0,
            TrackEventKind::Meta(MetaMessage::TrackName(track.name.as_bytes())),
        )];
        for region in &track.regions {
            let section = spec
                .sections
                .iter()
                .find(|section| section.id == region.section_id)
                .ok_or_else(|| {
                    CompileError::InvalidSpec(format!(
                        "track `{}` references unknown section `{}`",
                        track.id, region.section_id
                    ))
                })?;
            let section_tick = bar_to_tick(spec, section.start_bar);
            for cc in &region.cc {
                events.push((
                    section_tick + beat_to_tick(cc.beat),
                    1,
                    TrackEventKind::Midi {
                        channel,
                        message: MidiMessage::Controller {
                            controller: u7::new(cc.controller),
                            value: u7::new(cc.value),
                        },
                    },
                ));
            }
            for note in &region.notes {
                let start = section_tick + beat_to_tick(note.beat);
                let end = section_tick + beat_to_tick(note.beat + note.duration);
                events.push((
                    start,
                    2,
                    TrackEventKind::Midi {
                        channel,
                        message: MidiMessage::NoteOn {
                            key: u7::new(note.pitch),
                            vel: u7::new(note.velocity),
                        },
                    },
                ));
                events.push((
                    end,
                    0,
                    TrackEventKind::Midi {
                        channel,
                        message: MidiMessage::NoteOff {
                            key: u7::new(note.pitch),
                            vel: u7::new(0),
                        },
                    },
                ));
            }
        }
        tracks.push(delta_encode(events)?);
    }

    let smf = Smf {
        header: Header::new(
            Format::Parallel,
            Timing::Metrical(u15::new(MIDI_TICKS_PER_QUARTER)),
        ),
        tracks,
    };
    let mut bytes = Vec::new();
    smf.write_std(&mut bytes)
        .map_err(|error| CompileError::Encode(error.to_string()))?;
    Ok(bytes)
}

fn conductor_track(spec: &ExperimentalMusicSpec) -> Result<Vec<TrackEvent<'_>>, CompileError> {
    let mut events = vec![(
        0,
        0,
        TrackEventKind::Meta(MetaMessage::TrackName(b"Auto Studio Conductor")),
    )];
    for change in &spec.tempo_map {
        let tick = bar_to_tick(spec, change.bar);
        let micros = bpm_to_micros(change.bpm);
        events.push((
            tick,
            0,
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(micros))),
        ));
        events.push((
            tick,
            1,
            TrackEventKind::Meta(MetaMessage::TimeSignature(
                change.time_signature.numerator,
                u8::try_from(change.time_signature.denominator.trailing_zeros())
                    .map_err(|_| CompileError::TimelineOverflow)?,
                24,
                8,
            )),
        ));
    }
    for change in &spec.key_map {
        if let Some((accidentals, minor)) = midi_key_signature(&change.tonic, &change.mode) {
            events.push((
                bar_to_tick(spec, change.bar),
                2,
                TrackEventKind::Meta(MetaMessage::KeySignature(accidentals, minor)),
            ));
        }
    }
    for section in &spec.sections {
        events.push((
            bar_to_tick(spec, section.start_bar),
            3,
            TrackEventKind::Meta(MetaMessage::Marker(section.label.as_bytes())),
        ));
    }
    delta_encode(events)
}

fn delta_encode(mut events: Vec<TimedEvent<'_>>) -> Result<Vec<TrackEvent<'_>>, CompileError> {
    events.sort_by_key(|(tick, priority, _)| (*tick, *priority));
    let mut prior_tick = 0_u64;
    let mut track = Vec::with_capacity(events.len() + 1);
    for (tick, _, kind) in events {
        let delta = tick
            .checked_sub(prior_tick)
            .ok_or(CompileError::TimelineOverflow)?;
        if delta > MIDI_MAX_DELTA {
            return Err(CompileError::TimelineOverflow);
        }
        track.push(TrackEvent {
            delta: u28::new(u32::try_from(delta).map_err(|_| CompileError::TimelineOverflow)?),
            kind,
        });
        prior_tick = tick;
    }
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    Ok(track)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn beat_to_tick(beat: f64) -> u64 {
    (beat * f64::from(MIDI_TICKS_PER_QUARTER)).round() as u64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn bpm_to_micros(bpm: f64) -> u32 {
    (60_000_000.0 / bpm).round() as u32
}

fn bar_to_tick(spec: &ExperimentalMusicSpec, target_bar: u32) -> u64 {
    (1..target_bar)
        .map(|bar| ticks_per_bar(signature_at_bar(spec, bar)))
        .sum()
}

fn signature_at_bar(spec: &ExperimentalMusicSpec, bar: u32) -> TimeSignature {
    spec.tempo_map
        .iter()
        .rev()
        .find(|change| change.bar <= bar)
        .map_or(
            TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            |change| change.time_signature,
        )
}

fn ticks_per_bar(signature: TimeSignature) -> u64 {
    u64::from(MIDI_TICKS_PER_QUARTER) * u64::from(signature.numerator) * 4
        / u64::from(signature.denominator)
}

fn midi_key_signature(tonic: &str, mode: &str) -> Option<(i8, bool)> {
    let tonic = tonic.trim().to_ascii_lowercase();
    let minor = mode.trim().eq_ignore_ascii_case("minor");
    let accidentals = if minor {
        match tonic.as_str() {
            "ab" => -7,
            "eb" => -6,
            "bb" => -5,
            "f" => -4,
            "c" => -3,
            "g" => -2,
            "d" => -1,
            "a" => 0,
            "e" => 1,
            "b" => 2,
            "f#" => 3,
            "c#" => 4,
            "g#" => 5,
            "d#" => 6,
            "a#" => 7,
            _ => return None,
        }
    } else {
        match tonic.as_str() {
            "cb" => -7,
            "gb" => -6,
            "db" => -5,
            "ab" => -4,
            "eb" => -3,
            "bb" => -2,
            "f" => -1,
            "c" => 0,
            "g" => 1,
            "d" => 2,
            "a" => 3,
            "e" => 4,
            "b" => 5,
            "f#" => 6,
            "c#" => 7,
            _ => return None,
        }
    };
    Some((accidentals, minor))
}
