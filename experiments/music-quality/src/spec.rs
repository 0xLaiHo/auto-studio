use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::constants::{
    MAX_BAR, MAX_BPM, MAX_CC_PER_REGION, MAX_NOTES_PER_REGION, MAX_REGIONS_PER_TRACK,
    MAX_SECTION_BARS, MAX_SECTIONS, MAX_TIME_SIGNATURE_NUMERATOR, MAX_TRACKS, MIDI_MAX, MIN_BPM,
    SUPPORTED_TIME_SIGNATURE_DENOMINATORS,
};
use crate::error::SpecError;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentalMusicSpec {
    pub title: String,
    pub tempo_map: Vec<TempoChange>,
    pub key_map: Vec<KeyChange>,
    pub sections: Vec<MusicSection>,
    pub tracks: Vec<MusicTrack>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TempoChange {
    pub bar: u32,
    pub bpm: f64,
    pub time_signature: TimeSignature,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyChange {
    pub bar: u32,
    pub tonic: String,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MusicSection {
    pub id: String,
    pub label: String,
    pub start_bar: u32,
    pub length_bars: u32,
    pub intent: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MusicTrack {
    pub id: String,
    pub name: String,
    pub role: String,
    pub register: PitchRegister,
    pub instrument_hint: String,
    pub regions: Vec<MusicRegion>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PitchRegister {
    pub low: u8,
    pub high: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MusicRegion {
    pub section_id: String,
    #[serde(default)]
    pub notes: Vec<MidiNote>,
    #[serde(default)]
    pub cc: Vec<ControlChange>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MidiNote {
    pub beat: f64,
    pub duration: f64,
    pub pitch: u8,
    pub velocity: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlChange {
    pub beat: f64,
    pub controller: u8,
    pub value: u8,
}

impl ExperimentalMusicSpec {
    /// Parses strict JSON and validates musical and resource invariants.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError`] when JSON is malformed, contains unknown fields,
    /// or violates the frozen Q0 contract.
    pub fn parse_and_validate(input: &str) -> Result<Self, SpecError> {
        let spec: Self = serde_json::from_str(input)?;
        let violations = spec.violations();
        if violations.is_empty() {
            Ok(spec)
        } else {
            Err(SpecError::Validation(violations.join("; ")))
        }
    }

    #[must_use]
    pub fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        validate_nonempty("title", &self.title, &mut violations);
        self.validate_maps(&mut violations);
        let sections = self.validate_sections(&mut violations);
        self.validate_tracks(&sections, &mut violations);
        violations
    }

    fn validate_maps(&self, violations: &mut Vec<String>) {
        if self.tempo_map.is_empty() || self.tempo_map.first().is_none_or(|item| item.bar != 1) {
            violations.push("tempo_map must start at bar 1".to_owned());
        }
        let mut prior_bar = 0;
        for item in &self.tempo_map {
            if item.bar <= prior_bar || item.bar > MAX_BAR {
                violations.push(format!(
                    "tempo_map bar {} is not strictly increasing",
                    item.bar
                ));
            }
            if !item.bpm.is_finite() || !(MIN_BPM..=MAX_BPM).contains(&item.bpm) {
                violations.push(format!(
                    "tempo {} is outside {MIN_BPM}..={MAX_BPM} BPM",
                    item.bpm
                ));
            }
            if item.time_signature.numerator == 0
                || item.time_signature.numerator > MAX_TIME_SIGNATURE_NUMERATOR
                || !SUPPORTED_TIME_SIGNATURE_DENOMINATORS.contains(&item.time_signature.denominator)
            {
                violations.push(format!(
                    "unsupported time signature {}/{}",
                    item.time_signature.numerator, item.time_signature.denominator
                ));
            }
            prior_bar = item.bar;
        }
        if self.key_map.is_empty() || self.key_map.first().is_none_or(|item| item.bar != 1) {
            violations.push("key_map must start at bar 1".to_owned());
        }
        prior_bar = 0;
        for item in &self.key_map {
            if item.bar <= prior_bar || item.bar > MAX_BAR {
                violations.push(format!(
                    "key_map bar {} is not strictly increasing",
                    item.bar
                ));
            }
            validate_nonempty("key tonic", &item.tonic, violations);
            validate_nonempty("key mode", &item.mode, violations);
            prior_bar = item.bar;
        }
    }

    fn validate_sections<'a>(
        &'a self,
        violations: &mut Vec<String>,
    ) -> HashMap<&'a str, &'a MusicSection> {
        if self.sections.is_empty() || self.sections.len() > MAX_SECTIONS {
            violations.push(format!("sections count must be 1..={MAX_SECTIONS}"));
        }
        let mut ids = HashSet::new();
        let mut by_id = HashMap::new();
        let mut prior_end = 0_u32;
        for section in &self.sections {
            validate_nonempty("section id", &section.id, violations);
            if !ids.insert(section.id.as_str()) {
                violations.push(format!("duplicate section id `{}`", section.id));
            }
            if section.start_bar == 0 || section.start_bar > MAX_BAR {
                violations.push(format!("section `{}` has invalid start_bar", section.id));
            }
            if section.length_bars == 0 || section.length_bars > MAX_SECTION_BARS {
                violations.push(format!("section `{}` has invalid length_bars", section.id));
            }
            if section.start_bar <= prior_end {
                violations.push(format!(
                    "section `{}` overlaps the preceding section",
                    section.id
                ));
            }
            prior_end = section
                .start_bar
                .saturating_add(section.length_bars.saturating_sub(1));
            by_id.insert(section.id.as_str(), section);
        }
        by_id
    }

    fn validate_tracks(
        &self,
        sections: &HashMap<&str, &MusicSection>,
        violations: &mut Vec<String>,
    ) {
        if self.tracks.is_empty() || self.tracks.len() > MAX_TRACKS {
            violations.push(format!("tracks count must be 1..={MAX_TRACKS}"));
        }
        let fallback_beats = self
            .sections
            .iter()
            .map(|section| self.section_duration_beats(section))
            .fold(0.0_f64, f64::max);
        let mut ids = HashSet::new();
        for track in &self.tracks {
            if !ids.insert(track.id.as_str()) {
                violations.push(format!("duplicate track id `{}`", track.id));
            }
            if track.register.low > track.register.high || track.register.high > MIDI_MAX {
                violations.push(format!("track `{}` has invalid register", track.id));
            }
            if track.regions.len() > MAX_REGIONS_PER_TRACK {
                violations.push(format!("track `{}` has too many regions", track.id));
            }
            let mut region_sections = HashSet::new();
            for region in &track.regions {
                if !region_sections.insert(region.section_id.as_str()) {
                    violations.push(format!(
                        "track `{}` has duplicate region for section `{}`",
                        track.id, region.section_id
                    ));
                }
                let duration_beats = sections
                    .get(region.section_id.as_str())
                    .map_or(fallback_beats, |section| {
                        self.section_duration_beats(section)
                    });
                if !sections.contains_key(region.section_id.as_str()) {
                    violations.push(format!(
                        "track `{}` region references unknown section `{}`",
                        track.id, region.section_id
                    ));
                }
                Self::validate_region(track, region, duration_beats, violations);
            }
        }
    }

    fn validate_region(
        track: &MusicTrack,
        region: &MusicRegion,
        duration_beats: f64,
        violations: &mut Vec<String>,
    ) {
        if region.notes.len() > MAX_NOTES_PER_REGION {
            violations.push(format!("track `{}` region has too many notes", track.id));
        }
        if region.cc.len() > MAX_CC_PER_REGION {
            violations.push(format!(
                "track `{}` region has too many CC events",
                track.id
            ));
        }
        for (index, note) in region.notes.iter().enumerate() {
            if !note.beat.is_finite()
                || !note.duration.is_finite()
                || note.beat < 0.0
                || note.duration <= 0.0
                || note.beat + note.duration > duration_beats
            {
                violations.push(format!(
                    "track `{}` note {index} is outside section duration",
                    track.id
                ));
            }
            if !(track.register.low..=track.register.high).contains(&note.pitch) {
                violations.push(format!(
                    "track `{}` note {index} pitch {} is outside track register {}..={}",
                    track.id, note.pitch, track.register.low, track.register.high
                ));
            }
            if note.velocity == 0 || note.velocity > MIDI_MAX {
                violations.push(format!(
                    "track `{}` note {index} has invalid velocity",
                    track.id
                ));
            }
        }
        for (index, cc) in region.cc.iter().enumerate() {
            if !cc.beat.is_finite() || cc.beat < 0.0 || cc.beat > duration_beats {
                violations.push(format!(
                    "track `{}` CC {index} is outside section duration",
                    track.id
                ));
            }
            if cc.controller > MIDI_MAX || cc.value > MIDI_MAX {
                violations.push(format!(
                    "track `{}` CC {index} is outside MIDI range",
                    track.id
                ));
            }
        }
    }

    fn section_duration_beats(&self, section: &MusicSection) -> f64 {
        let signature = self
            .tempo_map
            .iter()
            .rev()
            .find(|change| change.bar <= section.start_bar)
            .map_or(
                TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                |change| change.time_signature,
            );
        f64::from(section.length_bars) * f64::from(signature.numerator) * 4.0
            / f64::from(signature.denominator)
    }
}

fn validate_nonempty(label: &str, value: &str, violations: &mut Vec<String>) {
    if value.trim().is_empty() {
        violations.push(format!("{label} must not be empty"));
    }
}
