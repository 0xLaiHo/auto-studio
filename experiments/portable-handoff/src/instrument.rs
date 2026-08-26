use std::collections::{HashMap, HashSet};

use autostudio_music_quality::{ExperimentalMusicSpec, MusicTrack};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{
    ASSIGNMENTS_SCHEMA_VERSION, CATALOG_JSON, GM_PERCUSSION_CHANNEL, MIDI_CHANNEL_MAX,
    MIDI_CHANNEL_MIN, MIDI_MAX,
};
use crate::error::InstrumentError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentLibrary {
    pub id: String,
    pub name: String,
    pub sha256: String,
    pub license_decision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentCatalog {
    pub schema_version: String,
    pub mapping_policy: String,
    pub library: InstrumentLibrary,
    pub profiles: Vec<InstrumentProfile>,
    pub fallback_profile_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentProfile {
    pub id: String,
    pub match_terms: Vec<String>,
    pub midi: MidiInstrumentProfile,
    pub soundfont_bank: u16,
    pub gm_name: String,
    pub preset_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MidiInstrumentProfile {
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_channel: Option<u8>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentSource {
    InstrumentHint,
    Role,
    TrackName,
    Fallback,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentAssignment {
    pub track_id: String,
    pub track_name: String,
    pub profile_id: String,
    pub source: AssignmentSource,
    pub midi_channel: u8,
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
    pub soundfont_bank: u16,
    pub gm_name: String,
    pub preset_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentAssignmentManifest {
    pub schema_version: String,
    pub catalog_schema_version: String,
    pub catalog_sha256: String,
    pub mapping_policy: String,
    pub library: InstrumentLibrary,
    pub assignments: Vec<InstrumentAssignment>,
}

impl InstrumentCatalog {
    /// Parses and validates a portable instrument catalog.
    ///
    /// # Errors
    ///
    /// Returns [`InstrumentError`] for malformed JSON or a catalog that cannot
    /// produce deterministic General MIDI assignments.
    pub fn parse_and_validate(input: &str) -> Result<Self, InstrumentError> {
        let catalog: Self = serde_json::from_str(input)?;
        let violations = catalog.violations();
        if violations.is_empty() {
            Ok(catalog)
        } else {
            Err(InstrumentError::Validation(violations.join("; ")))
        }
    }

    #[must_use]
    pub fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        validate_nonempty(
            "catalog schema_version",
            &self.schema_version,
            &mut violations,
        );
        validate_nonempty("mapping_policy", &self.mapping_policy, &mut violations);
        validate_nonempty("library id", &self.library.id, &mut violations);
        validate_nonempty("library name", &self.library.name, &mut violations);
        validate_nonempty(
            "library license_decision",
            &self.library.license_decision,
            &mut violations,
        );
        if self.library.sha256.len() != 64
            || !self
                .library
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            violations.push("library sha256 must contain 64 hexadecimal characters".to_owned());
        }
        if self.profiles.is_empty() {
            violations.push("instrument profiles must not be empty".to_owned());
        }

        let mut profile_ids = HashSet::new();
        let mut match_terms = HashMap::new();
        for profile in &self.profiles {
            validate_nonempty("instrument profile id", &profile.id, &mut violations);
            if !profile_ids.insert(profile.id.as_str()) {
                violations.push(format!("duplicate instrument profile `{}`", profile.id));
            }
            validate_nonempty("GM name", &profile.gm_name, &mut violations);
            validate_nonempty("preset name", &profile.preset_name, &mut violations);
            if profile.match_terms.is_empty() {
                violations.push(format!(
                    "instrument profile `{}` has no match terms",
                    profile.id
                ));
            }
            for term in &profile.match_terms {
                let normalized = normalize(term);
                if normalized.is_empty() {
                    violations.push(format!(
                        "instrument profile `{}` has an empty match term",
                        profile.id
                    ));
                } else if let Some(existing) = match_terms.insert(normalized, profile.id.as_str()) {
                    violations.push(format!(
                        "instrument profiles `{existing}` and `{}` share a match term",
                        profile.id
                    ));
                }
            }
            if profile.midi.bank_msb > MIDI_MAX
                || profile.midi.bank_lsb > MIDI_MAX
                || profile.midi.program > MIDI_MAX
            {
                violations.push(format!(
                    "instrument profile `{}` has MIDI values outside 0..={MIDI_MAX}",
                    profile.id
                ));
            }
            if profile
                .midi
                .preferred_channel
                .is_some_and(|channel| !(MIDI_CHANNEL_MIN..=MIDI_CHANNEL_MAX).contains(&channel))
            {
                violations.push(format!(
                    "instrument profile `{}` has an invalid preferred MIDI channel",
                    profile.id
                ));
            }
        }
        if !profile_ids.contains(self.fallback_profile_id.as_str()) {
            violations.push(format!(
                "fallback instrument profile `{}` does not exist",
                self.fallback_profile_id
            ));
        }
        violations
    }

    fn profile(&self, id: &str) -> Option<&InstrumentProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    fn resolve_profile<'a>(
        &'a self,
        track: &MusicTrack,
    ) -> Result<(&'a InstrumentProfile, AssignmentSource), InstrumentError> {
        if let Some(profile) = best_match(&self.profiles, &track.instrument_hint) {
            return Ok((profile, AssignmentSource::InstrumentHint));
        }
        if let Some(profile) = best_match(&self.profiles, &track.role) {
            return Ok((profile, AssignmentSource::Role));
        }
        if let Some(profile) = best_match(&self.profiles, &track.name) {
            return Ok((profile, AssignmentSource::TrackName));
        }
        Ok((
            self.profile(&self.fallback_profile_id).ok_or_else(|| {
                InstrumentError::Validation("fallback profile disappeared".to_owned())
            })?,
            AssignmentSource::Fallback,
        ))
    }
}

/// Resolves all semantic tracks with the versioned portable catalog.
///
/// # Errors
///
/// Returns [`InstrumentError`] when the catalog is invalid or the General MIDI
/// channel budget is exhausted.
pub fn resolve_instrument_assignments(
    spec: &ExperimentalMusicSpec,
) -> Result<InstrumentAssignmentManifest, InstrumentError> {
    let catalog = InstrumentCatalog::parse_and_validate(CATALOG_JSON)?;
    resolve_with_catalog(spec, &catalog)
}

/// Resolves all semantic tracks with an explicitly supplied catalog.
///
/// # Errors
///
/// Returns [`InstrumentError`] when the General MIDI channel budget is
/// exhausted.
pub fn resolve_with_catalog(
    spec: &ExperimentalMusicSpec,
    catalog: &InstrumentCatalog,
) -> Result<InstrumentAssignmentManifest, InstrumentError> {
    let resolved = spec
        .tracks
        .iter()
        .map(|track| catalog.resolve_profile(track).map(|value| (track, value)))
        .collect::<Result<Vec<_>, _>>()?;
    let reserved_channels = resolved
        .iter()
        .filter_map(|(_, (profile, _))| profile.midi.preferred_channel)
        .collect::<HashSet<_>>();
    let available_channels = (MIDI_CHANNEL_MIN..=MIDI_CHANNEL_MAX)
        .filter(|channel| *channel != GM_PERCUSSION_CHANNEL && !reserved_channels.contains(channel))
        .collect::<Vec<_>>();
    let mut next_channel = 0_usize;
    let mut assignments = Vec::with_capacity(resolved.len());

    for (track, (profile, source)) in resolved {
        let midi_channel = if let Some(channel) = profile.midi.preferred_channel {
            channel
        } else {
            let channel = available_channels
                .get(next_channel)
                .copied()
                .ok_or_else(|| InstrumentError::ChannelExhausted {
                    track_id: track.id.clone(),
                })?;
            next_channel += 1;
            channel
        };
        assignments.push(InstrumentAssignment {
            track_id: track.id.clone(),
            track_name: track.name.clone(),
            profile_id: profile.id.clone(),
            source,
            midi_channel,
            bank_msb: profile.midi.bank_msb,
            bank_lsb: profile.midi.bank_lsb,
            program: profile.midi.program,
            soundfont_bank: profile.soundfont_bank,
            gm_name: profile.gm_name.clone(),
            preset_name: profile.preset_name.clone(),
        });
    }

    Ok(InstrumentAssignmentManifest {
        schema_version: ASSIGNMENTS_SCHEMA_VERSION.to_owned(),
        catalog_schema_version: catalog.schema_version.clone(),
        catalog_sha256: hex::encode(Sha256::digest(CATALOG_JSON.as_bytes())),
        mapping_policy: catalog.mapping_policy.clone(),
        library: catalog.library.clone(),
        assignments,
    })
}

fn best_match<'a>(profiles: &'a [InstrumentProfile], value: &str) -> Option<&'a InstrumentProfile> {
    let normalized = format!(" {} ", normalize(value));
    let mut best: Option<(&InstrumentProfile, (usize, usize))> = None;
    for profile in profiles {
        for term in &profile.match_terms {
            let term = normalize(term);
            if term.is_empty() || !normalized.contains(&format!(" {term} ")) {
                continue;
            }
            let score = (term.split_whitespace().count(), term.len());
            let should_replace = best.as_ref().is_none_or(|(current, current_score)| {
                score > *current_score || (score == *current_score && profile.id < current.id)
            });
            if should_replace {
                best = Some((profile, score));
            }
        }
    }
    best.map(|(profile, _)| profile)
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_nonempty(label: &str, value: &str, violations: &mut Vec<String>) {
    if value.trim().is_empty() {
        violations.push(format!("{label} must not be empty"));
    }
}
