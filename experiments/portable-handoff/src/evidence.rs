use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use autostudio_music_quality::ExperimentalMusicSpec;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::compile_portable_smf;
use crate::constants::{
    ASSIGNMENTS_FILE, EVIDENCE_SCHEMA_VERSION, MANIFEST_FILE, MIDI_FILE, SPEC_FILE,
};
use crate::error::HandoffError;
use crate::instrument::resolve_instrument_assignments;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceManifest {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// Writes the normalized spec, assignment manifest and portable MIDI atomically.
///
/// Every artifact is hashed from the staged copy that is subsequently renamed.
///
/// # Errors
///
/// Returns [`HandoffError`] for invalid assignments, MIDI encoding,
/// serialization or filesystem failures.
pub fn write_portable_handoff(
    output_dir: &Path,
    spec: &ExperimentalMusicSpec,
) -> Result<EvidenceManifest, HandoffError> {
    fs::create_dir_all(output_dir)?;
    let assignments = resolve_instrument_assignments(spec)?;
    let spec_bytes = serde_json::to_vec_pretty(spec)?;
    let assignment_bytes = serde_json::to_vec_pretty(&assignments)?;
    let midi = compile_portable_smf(spec, &assignments)?;
    let artifacts = vec![
        write_hashed_artifact(output_dir, SPEC_FILE, &spec_bytes)?,
        write_hashed_artifact(output_dir, ASSIGNMENTS_FILE, &assignment_bytes)?,
        write_hashed_artifact(output_dir, MIDI_FILE, &midi)?,
    ];
    let manifest = EvidenceManifest {
        schema_version: EVIDENCE_SCHEMA_VERSION.to_owned(),
        generated_at: Utc::now(),
        artifacts,
    };
    write_atomic(
        output_dir,
        MANIFEST_FILE,
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

fn write_hashed_artifact(
    output_dir: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<ArtifactRecord, HandoffError> {
    let staging = staging_path(output_dir, name);
    write_file_synced(&staging, bytes)?;
    let staged_bytes = fs::read(&staging)?;
    let record = ArtifactRecord {
        path: name.to_owned(),
        bytes: u64::try_from(staged_bytes.len()).unwrap_or(u64::MAX),
        sha256: hex::encode(Sha256::digest(&staged_bytes)),
    };
    fs::rename(staging, output_dir.join(name))?;
    Ok(record)
}

fn write_atomic(output_dir: &Path, name: &str, bytes: &[u8]) -> Result<(), HandoffError> {
    let staging = staging_path(output_dir, name);
    write_file_synced(&staging, bytes)?;
    fs::rename(staging, output_dir.join(name))?;
    Ok(())
}

fn write_file_synced(path: &Path, bytes: &[u8]) -> Result<(), HandoffError> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn staging_path(output_dir: &Path, name: &str) -> PathBuf {
    output_dir.join(format!(".{name}.staging"))
}
