use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::compile_to_smf;
use crate::constants::{EVIDENCE_SCHEMA_VERSION, MANIFEST_FILE, MIDI_FILE, NORMALIZED_SPEC_FILE};
use crate::error::ExperimentError;
use crate::spec::ExperimentalMusicSpec;

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

/// Writes normalized JSON, compiled MIDI and their integrity manifest.
///
/// Each payload is hashed from its staging copy before atomic rename. The
/// manifest never reads credentials and contains no Provider request data.
///
/// # Errors
///
/// Returns [`ExperimentError`] for invalid output, encoding, serialization or
/// filesystem failures.
pub fn write_compilation_evidence(
    output_dir: &Path,
    spec: &ExperimentalMusicSpec,
) -> Result<EvidenceManifest, ExperimentError> {
    fs::create_dir_all(output_dir)?;
    let normalized = serde_json::to_vec_pretty(spec)?;
    let midi = compile_to_smf(spec)?;
    let artifacts = vec![
        write_hashed_artifact(output_dir, NORMALIZED_SPEC_FILE, &normalized)?,
        write_hashed_artifact(output_dir, MIDI_FILE, &midi)?,
    ];
    let manifest = EvidenceManifest {
        schema_version: EVIDENCE_SCHEMA_VERSION.to_owned(),
        generated_at: Utc::now(),
        artifacts,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    write_atomic(output_dir, MANIFEST_FILE, &bytes)?;
    Ok(manifest)
}

pub(crate) fn write_hashed_artifact(
    output_dir: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<ArtifactRecord, ExperimentError> {
    let staging = staging_path(output_dir, name);
    write_file_synced(&staging, bytes)?;
    let staged_bytes = fs::read(&staging)?;
    let record = ArtifactRecord {
        path: name.to_owned(),
        bytes: u64::try_from(staged_bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256(&staged_bytes),
    };
    fs::rename(staging, output_dir.join(name))?;
    Ok(record)
}

pub(crate) fn record_existing_artifact(
    output_dir: &Path,
    name: &str,
) -> Result<ArtifactRecord, ExperimentError> {
    let bytes = fs::read(output_dir.join(name))?;
    Ok(ArtifactRecord {
        path: name.to_owned(),
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256(&bytes),
    })
}

pub(crate) fn write_atomic(
    output_dir: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<(), ExperimentError> {
    let staging = staging_path(output_dir, name);
    write_file_synced(&staging, bytes)?;
    fs::rename(staging, output_dir.join(name))?;
    Ok(())
}

fn write_file_synced(path: &Path, bytes: &[u8]) -> Result<(), ExperimentError> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn staging_path(output_dir: &Path, name: &str) -> PathBuf {
    output_dir.join(format!(".{name}.staging"))
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
