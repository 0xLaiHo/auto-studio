use std::collections::HashSet;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ExperimentError;
use crate::evidence::{ArtifactRecord, sha256, write_atomic, write_hashed_artifact};
use crate::runner::{ExperimentRun, RunMode};

const BLIND_SCHEMA_VERSION: &str = "q0-blind-package-v1";
const REQUIRED_ARTIFACTS: [&str; 3] = ["brief.json", "spec.json", "composition.mid"];
const EVALUATION_HEADER: &str = "candidate_id,evaluator_id,started_at,ended_at,keep,brief_match,structure,harmony,melody,groove,orchestration,severe_structural_error,sections_deleted,sections_rewritten,tracks_deleted,tracks_added,regions_rewritten,notes_changed,operations,edited_midi_sha256,notes\n";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlindPackageManifest {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub candidates: Vec<BlindCandidate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlindCandidate {
    pub candidate_id: String,
    pub brief_id: String,
    pub brief_level: String,
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PrivateBlindMap {
    schema_version: String,
    generated_at: DateTime<Utc>,
    entries: Vec<PrivateBlindEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PrivateBlindEntry {
    candidate_id: String,
    brief_id: String,
    mode: RunMode,
    source: String,
}

/// Copies completed formal artifacts into an evaluator-safe package and writes
/// the mode mapping outside that package.
///
/// # Errors
///
/// Returns [`ExperimentError`] when evidence is malformed, incomplete, has a
/// duplicate candidate identity, or fails its recorded hash.
pub fn prepare_blind_package(
    evidence_root: &Path,
    output_root: &Path,
) -> Result<BlindPackageManifest, ExperimentError> {
    let evaluator_root = output_root.join("evaluator");
    fs::create_dir_all(&evaluator_root)?;
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    let mut private_entries = Vec::new();

    for mode_dir in ["mode-a", "mode-b", "mode-c"] {
        let mode_root = evidence_root.join(mode_dir);
        if !mode_root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&mode_root)? {
            let source = entry?.path();
            if !source.is_dir() || !source.join("run.json").is_file() {
                continue;
            }
            append_candidate(
                evidence_root,
                &evaluator_root,
                &source,
                &mut seen,
                &mut candidates,
                &mut private_entries,
            )?;
        }
    }
    if candidates.is_empty() {
        return Err(ExperimentError::InvalidInput(
            "no completed formal candidates were found".to_owned(),
        ));
    }
    candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    private_entries.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let generated_at = Utc::now();
    let manifest = BlindPackageManifest {
        schema_version: BLIND_SCHEMA_VERSION.to_owned(),
        generated_at,
        candidates,
    };
    let private_map = PrivateBlindMap {
        schema_version: BLIND_SCHEMA_VERSION.to_owned(),
        generated_at,
        entries: private_entries,
    };
    write_atomic(
        &evaluator_root,
        "manifest.json",
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    write_atomic(
        output_root,
        "blind-map.private.json",
        &serde_json::to_vec_pretty(&private_map)?,
    )?;
    let evaluation =
        manifest
            .candidates
            .iter()
            .fold(EVALUATION_HEADER.to_owned(), |mut csv, candidate| {
                csv.push_str(&candidate.candidate_id);
                csv.push_str(",,,,,,,,,,,,,,,,,,,,\n");
                csv
            });
    write_atomic(&evaluator_root, "evaluation.csv", evaluation.as_bytes())?;
    Ok(manifest)
}

fn append_candidate(
    evidence_root: &Path,
    evaluator_root: &Path,
    source: &Path,
    seen: &mut HashSet<String>,
    candidates: &mut Vec<BlindCandidate>,
    private_entries: &mut Vec<PrivateBlindEntry>,
) -> Result<(), ExperimentError> {
    let run: ExperimentRun = serde_json::from_slice(&fs::read(source.join("run.json"))?)?;
    if run.status != "completed" || !run.schema_valid || !run.compiled {
        return Ok(());
    }
    if !seen.insert(run.candidate_id.clone()) {
        return Err(ExperimentError::InvalidInput(format!(
            "duplicate blind candidate identity `{}`",
            run.candidate_id
        )));
    }
    let candidate_root = evaluator_root.join(&run.candidate_id);
    fs::create_dir_all(&candidate_root)?;
    let mut artifacts = Vec::new();
    for name in REQUIRED_ARTIFACTS {
        let expected = run
            .artifacts
            .iter()
            .find(|artifact| artifact.path == name)
            .ok_or_else(|| {
                ExperimentError::InvalidInput(format!(
                    "{} is missing recorded artifact `{name}`",
                    source.display()
                ))
            })?;
        let bytes = fs::read(source.join(name))?;
        if sha256(&bytes) != expected.sha256 {
            return Err(ExperimentError::InvalidInput(format!(
                "{} failed its recorded SHA-256",
                source.join(name).display()
            )));
        }
        artifacts.push(write_hashed_artifact(&candidate_root, name, &bytes)?);
    }
    candidates.push(BlindCandidate {
        candidate_id: run.candidate_id.clone(),
        brief_id: run.brief_id.clone(),
        brief_level: run.brief_level.clone(),
        artifacts,
    });
    private_entries.push(PrivateBlindEntry {
        candidate_id: run.candidate_id,
        brief_id: run.brief_id,
        mode: run.mode,
        source: relative_source(evidence_root, source),
    });
    Ok(())
}

fn relative_source(root: &Path, source: &Path) -> String {
    source
        .strip_prefix(root)
        .unwrap_or(source)
        .to_string_lossy()
        .into_owned()
}
