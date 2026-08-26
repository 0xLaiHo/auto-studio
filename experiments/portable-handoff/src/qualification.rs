use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{
    ASSIGNMENTS_FILE, EVIDENCE_SCHEMA_VERSION, MANIFEST_FILE, MIDI_FILE, QUALIFICATION_PLAN_FILE,
    QUALIFICATION_PLAN_SCHEMA_VERSION, QUALIFICATION_REQUIRED_CHECKS, QUALIFICATION_RESULTS_FILE,
    QUALIFICATION_RESULTS_SCHEMA_VERSION, QUALIFICATION_SUMMARY_SCHEMA_VERSION,
    QUALIFICATION_TARGETS_SCHEMA_VERSION, SPEC_FILE,
};
use crate::error::{HandoffError, QualificationError};
use crate::evidence::{ArtifactRecord, EvidenceManifest};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationTargets {
    pub schema_version: String,
    pub frozen_at: String,
    pub targets: Vec<QualificationTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationTarget {
    pub id: String,
    pub product: String,
    pub exact_version: Option<String>,
    pub platform: Option<String>,
    pub readiness: TargetReadiness,
    pub blocked_reason: Option<String>,
    pub required_for_mvp: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetReadiness {
    Ready,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationPlan {
    pub schema_version: String,
    pub handoff_manifest_sha256: String,
    pub handoff_artifacts: Vec<ArtifactRecord>,
    pub targets_frozen_at: String,
    pub targets: Vec<QualificationTarget>,
    pub required_checks: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationResults {
    pub schema_version: String,
    pub plan_sha256: String,
    pub results: Vec<TargetResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetResult {
    pub target_id: String,
    pub outcome: QualificationOutcome,
    pub observed_version: Option<String>,
    pub executable_sha256: Option<String>,
    pub checks: Option<QualificationChecks>,
    pub evidence: Option<QualificationEvidence>,
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationOutcome {
    NotRun,
    Pass,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationChecks {
    pub import_without_repair: CheckStatus,
    pub semantic_tracks_preserved: CheckStatus,
    pub tempo_meter_preserved: CheckStatus,
    pub midi_events_preserved_before_edit: CheckStatus,
    pub markers: MarkerObservation,
    pub program_change: ProgramChangeObservation,
    pub save_reopen: CheckStatus,
    pub intentional_edit_export: CheckStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerObservation {
    Preserved,
    NotExposed,
    Lost,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramChangeObservation {
    Honored,
    Ignored,
    Remapped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationEvidence {
    pub screenshot: EvidenceArtifact,
    pub saved_project: EvidenceArtifact,
    pub edited_midi: EvidenceArtifact,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationSummary {
    pub schema_version: String,
    pub plan_sha256: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub not_run: usize,
    pub required_targets: usize,
    pub all_required_targets_passed: bool,
    pub targets: Vec<TargetSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSummary {
    pub target_id: String,
    pub product: String,
    pub required_for_mvp: bool,
    pub outcome: QualificationOutcome,
}

/// Freezes the DAW targets and exact handoff hashes into a deterministic plan.
///
/// It also writes a `not_run` result template bound to the plan hash. Existing
/// handoff artifacts are verified before either qualification file is written.
///
/// # Errors
///
/// Returns [`HandoffError`] when the handoff package or target file is invalid,
/// an artifact hash does not match, or an output cannot be written.
pub fn prepare_qualification_matrix(
    handoff_dir: &Path,
    targets_file: &Path,
    output_dir: &Path,
) -> Result<(QualificationPlan, QualificationResults), HandoffError> {
    let (manifest, manifest_sha256) = verify_handoff_manifest(handoff_dir)?;
    let targets: QualificationTargets = read_json(targets_file)?;
    validate_targets(&targets)?;

    let plan = QualificationPlan {
        schema_version: QUALIFICATION_PLAN_SCHEMA_VERSION.to_owned(),
        handoff_manifest_sha256: manifest_sha256,
        handoff_artifacts: manifest.artifacts,
        targets_frozen_at: targets.frozen_at,
        targets: targets.targets,
        required_checks: QUALIFICATION_REQUIRED_CHECKS
            .iter()
            .map(ToString::to_string)
            .collect(),
    };
    let plan_bytes = serde_json::to_vec_pretty(&plan)?;
    let plan_sha256 = sha256(&plan_bytes);
    let results = QualificationResults {
        schema_version: QUALIFICATION_RESULTS_SCHEMA_VERSION.to_owned(),
        plan_sha256,
        results: plan
            .targets
            .iter()
            .map(|target| TargetResult {
                target_id: target.id.clone(),
                outcome: QualificationOutcome::NotRun,
                observed_version: None,
                executable_sha256: None,
                checks: None,
                evidence: None,
                notes: target.blocked_reason.clone(),
            })
            .collect(),
    };

    fs::create_dir_all(output_dir)?;
    write_atomic(output_dir, QUALIFICATION_PLAN_FILE, &plan_bytes)?;
    write_atomic(
        output_dir,
        QUALIFICATION_RESULTS_FILE,
        &serde_json::to_vec_pretty(&results)?,
    )?;
    Ok((plan, results))
}

/// Verifies a completed matrix and writes a deterministic summary.
///
/// A `pass` result is accepted only for a ready, exact-version target with all
/// required checks, valid screenshot/project/edited-MIDI hashes, and an edited
/// MIDI file whose channel events differ from the source handoff.
///
/// # Errors
///
/// Returns [`HandoffError`] when bindings, checks or evidence are incomplete or
/// inconsistent. A valid `fail` or `not_run` outcome is summarized normally.
pub fn verify_qualification_matrix(
    handoff_dir: &Path,
    plan_file: &Path,
    results_file: &Path,
    evidence_root: &Path,
    output_file: &Path,
) -> Result<QualificationSummary, HandoffError> {
    let plan_bytes = fs::read(plan_file)?;
    let plan: QualificationPlan = serde_json::from_slice(&plan_bytes)?;
    validate_plan(&plan)?;
    let plan_sha256 = sha256(&plan_bytes);
    let results: QualificationResults = read_json(results_file)?;
    validate_results_binding(&plan, &results, &plan_sha256)?;
    verify_plan_handoff_binding(handoff_dir, &plan)?;

    let source_midi = fs::read(handoff_dir.join(MIDI_FILE))?;
    let mut summaries = Vec::with_capacity(plan.targets.len());
    for (target, result) in plan.targets.iter().zip(&results.results) {
        validate_target_result(target, result, evidence_root, &source_midi)?;
        summaries.push(TargetSummary {
            target_id: target.id.clone(),
            product: target.product.clone(),
            required_for_mvp: target.required_for_mvp,
            outcome: result.outcome,
        });
    }

    let passed = summaries
        .iter()
        .filter(|result| result.outcome == QualificationOutcome::Pass)
        .count();
    let failed = summaries
        .iter()
        .filter(|result| result.outcome == QualificationOutcome::Fail)
        .count();
    let not_run = summaries
        .iter()
        .filter(|result| result.outcome == QualificationOutcome::NotRun)
        .count();
    let required_targets = summaries
        .iter()
        .filter(|result| result.required_for_mvp)
        .count();
    let all_required_targets_passed = summaries
        .iter()
        .all(|result| !result.required_for_mvp || result.outcome == QualificationOutcome::Pass);
    let summary = QualificationSummary {
        schema_version: QUALIFICATION_SUMMARY_SCHEMA_VERSION.to_owned(),
        plan_sha256,
        total: summaries.len(),
        passed,
        failed,
        not_run,
        required_targets,
        all_required_targets_passed,
        targets: summaries,
    };
    let output_dir = output_file.parent().ok_or_else(|| {
        QualificationError::InvalidInput("summary output needs a parent directory".to_owned())
    })?;
    fs::create_dir_all(output_dir)?;
    let output_name = output_file.file_name().ok_or_else(|| {
        QualificationError::InvalidInput("summary output needs a file name".to_owned())
    })?;
    write_atomic(
        output_dir,
        &output_name.to_string_lossy(),
        &serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(summary)
}

fn validate_targets(targets: &QualificationTargets) -> Result<(), QualificationError> {
    if targets.schema_version != QUALIFICATION_TARGETS_SCHEMA_VERSION {
        return Err(QualificationError::InvalidInput(format!(
            "unsupported qualification targets schema `{}`",
            targets.schema_version
        )));
    }
    validate_target_list(&targets.frozen_at, &targets.targets)
}

fn validate_target_list(
    frozen_at: &str,
    targets: &[QualificationTarget],
) -> Result<(), QualificationError> {
    require_non_empty("targets.frozen_at", frozen_at)?;
    if targets.is_empty() {
        return Err(QualificationError::InvalidInput(
            "qualification target list is empty".to_owned(),
        ));
    }
    let mut ids = HashSet::new();
    for target in targets {
        validate_target(target)?;
        if !ids.insert(target.id.as_str()) {
            return Err(QualificationError::InvalidInput(format!(
                "duplicate qualification target `{}`",
                target.id
            )));
        }
    }
    if !targets.iter().any(|target| target.required_for_mvp) {
        return Err(QualificationError::InvalidInput(
            "qualification target list has no required MVP target".to_owned(),
        ));
    }
    Ok(())
}

fn validate_target(target: &QualificationTarget) -> Result<(), QualificationError> {
    require_non_empty("target.id", &target.id)?;
    require_non_empty("target.product", &target.product)?;
    match target.readiness {
        TargetReadiness::Ready => {
            require_optional_non_empty("target.exact_version", target.exact_version.as_deref())?;
            require_optional_non_empty("target.platform", target.platform.as_deref())?;
            if target.blocked_reason.is_some() {
                return Err(QualificationError::InvalidInput(format!(
                    "ready target `{}` cannot have a blocked reason",
                    target.id
                )));
            }
        }
        TargetReadiness::Blocked => {
            require_optional_non_empty("target.blocked_reason", target.blocked_reason.as_deref())?;
            if let Some(version) = target.exact_version.as_deref() {
                require_non_empty("target.exact_version", version)?;
            }
            if let Some(platform) = target.platform.as_deref() {
                require_non_empty("target.platform", platform)?;
            }
        }
    }
    Ok(())
}

fn validate_plan(plan: &QualificationPlan) -> Result<(), QualificationError> {
    if plan.schema_version != QUALIFICATION_PLAN_SCHEMA_VERSION {
        return Err(QualificationError::InvalidInput(format!(
            "unsupported qualification plan schema `{}`",
            plan.schema_version
        )));
    }
    if !is_sha256(&plan.handoff_manifest_sha256) {
        return Err(QualificationError::InvalidInput(
            "handoff_manifest_sha256 is not a lowercase SHA-256".to_owned(),
        ));
    }
    if plan.required_checks != QUALIFICATION_REQUIRED_CHECKS {
        return Err(QualificationError::InvalidInput(
            "qualification plan required_checks differ from the verifier contract".to_owned(),
        ));
    }
    validate_target_list(&plan.targets_frozen_at, &plan.targets)?;
    validate_artifact_records(&plan.handoff_artifacts)
}

fn validate_results_binding(
    plan: &QualificationPlan,
    results: &QualificationResults,
    plan_sha256: &str,
) -> Result<(), QualificationError> {
    if results.schema_version != QUALIFICATION_RESULTS_SCHEMA_VERSION {
        return Err(QualificationError::InvalidInput(format!(
            "unsupported qualification results schema `{}`",
            results.schema_version
        )));
    }
    if results.plan_sha256 != plan_sha256 {
        return Err(QualificationError::HashMismatch {
            path: QUALIFICATION_PLAN_FILE.to_owned(),
            actual: plan_sha256.to_owned(),
            expected: results.plan_sha256.clone(),
        });
    }
    if results.results.len() != plan.targets.len() {
        return Err(QualificationError::InvalidInput(format!(
            "qualification result count {} does not match target count {}",
            results.results.len(),
            plan.targets.len()
        )));
    }
    for (target, result) in plan.targets.iter().zip(&results.results) {
        if target.id != result.target_id {
            return Err(QualificationError::InvalidInput(format!(
                "qualification result `{}` is out of order; expected `{}`",
                result.target_id, target.id
            )));
        }
    }
    Ok(())
}

fn validate_target_result(
    target: &QualificationTarget,
    result: &TargetResult,
    evidence_root: &Path,
    source_midi: &[u8],
) -> Result<(), HandoffError> {
    match result.outcome {
        QualificationOutcome::NotRun => {
            if result.observed_version.is_some()
                || result.executable_sha256.is_some()
                || result.checks.is_some()
                || result.evidence.is_some()
            {
                return Err(QualificationError::InvalidInput(format!(
                    "not-run target `{}` cannot carry observations or evidence",
                    target.id
                ))
                .into());
            }
            Ok(())
        }
        QualificationOutcome::Fail => {
            require_optional_non_empty("failed result notes", result.notes.as_deref())?;
            Ok(())
        }
        QualificationOutcome::Pass => {
            if target.readiness != TargetReadiness::Ready {
                return Err(QualificationError::InvalidInput(format!(
                    "blocked target `{}` cannot pass",
                    target.id
                ))
                .into());
            }
            let exact_version = target.exact_version.as_deref().ok_or_else(|| {
                QualificationError::InvalidInput(format!(
                    "ready target `{}` has no exact version",
                    target.id
                ))
            })?;
            if result.observed_version.as_deref() != Some(exact_version) {
                return Err(QualificationError::InvalidInput(format!(
                    "target `{}` observed version does not match frozen version `{exact_version}`",
                    target.id
                ))
                .into());
            }
            if !result.executable_sha256.as_deref().is_some_and(is_sha256) {
                return Err(QualificationError::InvalidInput(format!(
                    "target `{}` needs a lowercase executable SHA-256",
                    target.id
                ))
                .into());
            }
            let checks = result.checks.as_ref().ok_or_else(|| {
                QualificationError::InvalidInput(format!(
                    "target `{}` has no qualification checks",
                    target.id
                ))
            })?;
            validate_passing_checks(target, checks)?;
            let evidence = result.evidence.as_ref().ok_or_else(|| {
                QualificationError::InvalidInput(format!(
                    "target `{}` has no qualification evidence",
                    target.id
                ))
            })?;
            let screenshot = verify_evidence_artifact(evidence_root, &evidence.screenshot)?;
            if !is_supported_image(&screenshot) {
                return Err(QualificationError::InvalidInput(format!(
                    "target `{}` screenshot is not PNG or JPEG evidence",
                    target.id
                ))
                .into());
            }
            verify_evidence_artifact(evidence_root, &evidence.saved_project)?;
            let edited_midi = verify_evidence_artifact(evidence_root, &evidence.edited_midi)?;
            let edited_smf = midly::Smf::parse(&edited_midi).map_err(|error| {
                QualificationError::InvalidInput(format!(
                    "target `{}` edited MIDI is invalid: {error}",
                    target.id
                ))
            })?;
            let source_smf = midly::Smf::parse(source_midi).map_err(|error| {
                QualificationError::InvalidInput(format!("source handoff MIDI is invalid: {error}"))
            })?;
            if midi_event_fingerprint(&edited_smf) == midi_event_fingerprint(&source_smf) {
                return Err(QualificationError::InvalidInput(format!(
                    "target `{}` edited MIDI has no changed channel event",
                    target.id
                ))
                .into());
            }
            Ok(())
        }
    }
}

fn validate_passing_checks(
    target: &QualificationTarget,
    checks: &QualificationChecks,
) -> Result<(), QualificationError> {
    if checks.import_without_repair != CheckStatus::Passed
        || checks.semantic_tracks_preserved != CheckStatus::Passed
        || checks.tempo_meter_preserved != CheckStatus::Passed
        || checks.midi_events_preserved_before_edit != CheckStatus::Passed
        || checks.markers == MarkerObservation::Lost
        || checks.save_reopen != CheckStatus::Passed
        || checks.intentional_edit_export != CheckStatus::Passed
    {
        return Err(QualificationError::InvalidInput(format!(
            "target `{}` cannot pass because one or more required checks failed",
            target.id
        )));
    }
    Ok(())
}

fn verify_handoff_manifest(handoff_dir: &Path) -> Result<(EvidenceManifest, String), HandoffError> {
    let manifest_bytes = fs::read(handoff_dir.join(MANIFEST_FILE))?;
    let manifest: EvidenceManifest = serde_json::from_slice(&manifest_bytes)?;
    if manifest.schema_version != EVIDENCE_SCHEMA_VERSION {
        return Err(QualificationError::InvalidInput(format!(
            "unsupported handoff manifest schema `{}`",
            manifest.schema_version
        ))
        .into());
    }
    validate_artifact_records(&manifest.artifacts)?;
    verify_artifact_records(handoff_dir, &manifest.artifacts)?;
    Ok((manifest, sha256(&manifest_bytes)))
}

fn verify_plan_handoff_binding(
    handoff_dir: &Path,
    plan: &QualificationPlan,
) -> Result<(), HandoffError> {
    let (manifest, manifest_sha256) = verify_handoff_manifest(handoff_dir)?;
    if manifest_sha256 != plan.handoff_manifest_sha256 {
        return Err(QualificationError::HashMismatch {
            path: MANIFEST_FILE.to_owned(),
            actual: manifest_sha256,
            expected: plan.handoff_manifest_sha256.clone(),
        }
        .into());
    }
    if manifest.artifacts != plan.handoff_artifacts {
        return Err(QualificationError::InvalidInput(
            "handoff artifacts differ from the frozen qualification plan".to_owned(),
        )
        .into());
    }
    Ok(())
}

fn validate_artifact_records(artifacts: &[ArtifactRecord]) -> Result<(), QualificationError> {
    let expected = [SPEC_FILE, ASSIGNMENTS_FILE, MIDI_FILE];
    if artifacts.len() != expected.len() {
        return Err(QualificationError::InvalidInput(format!(
            "handoff manifest contains {} artifacts, expected {}",
            artifacts.len(),
            expected.len()
        )));
    }
    for (artifact, expected_path) in artifacts.iter().zip(expected) {
        if artifact.path != expected_path {
            return Err(QualificationError::InvalidInput(format!(
                "handoff artifact `{}` is out of order; expected `{expected_path}`",
                artifact.path
            )));
        }
        if artifact.bytes == 0 || !is_sha256(&artifact.sha256) {
            return Err(QualificationError::InvalidInput(format!(
                "handoff artifact `{}` has invalid size or SHA-256",
                artifact.path
            )));
        }
    }
    Ok(())
}

fn verify_artifact_records(root: &Path, artifacts: &[ArtifactRecord]) -> Result<(), HandoffError> {
    for artifact in artifacts {
        let bytes = fs::read(root.join(&artifact.path))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != artifact.bytes {
            return Err(QualificationError::InvalidInput(format!(
                "handoff artifact `{}` size does not match its manifest",
                artifact.path
            ))
            .into());
        }
        let actual = sha256(&bytes);
        if actual != artifact.sha256 {
            return Err(QualificationError::HashMismatch {
                path: artifact.path.clone(),
                actual,
                expected: artifact.sha256.clone(),
            }
            .into());
        }
    }
    Ok(())
}

fn verify_evidence_artifact(
    root: &Path,
    artifact: &EvidenceArtifact,
) -> Result<Vec<u8>, HandoffError> {
    let path = safe_evidence_path(root, &artifact.path)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|_| QualificationError::MissingEvidence(root.display().to_string()))?;
    let canonical_path = fs::canonicalize(&path)
        .map_err(|_| QualificationError::MissingEvidence(artifact.path.clone()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(QualificationError::UnsafeEvidencePath(artifact.path.clone()).into());
    }
    let bytes = fs::read(&canonical_path)?;
    if bytes.is_empty() || u64::try_from(bytes.len()).unwrap_or(u64::MAX) != artifact.bytes {
        return Err(QualificationError::InvalidInput(format!(
            "qualification evidence `{}` has an invalid size",
            artifact.path
        ))
        .into());
    }
    let actual = sha256(&bytes);
    if actual != artifact.sha256 {
        return Err(QualificationError::HashMismatch {
            path: artifact.path.clone(),
            actual,
            expected: artifact.sha256.clone(),
        }
        .into());
    }
    Ok(bytes)
}

fn safe_evidence_path(root: &Path, relative: &str) -> Result<PathBuf, QualificationError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(QualificationError::UnsafeEvidencePath(relative.to_owned()));
    }
    Ok(root.join(path))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, HandoffError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn require_non_empty(field: &str, value: &str) -> Result<(), QualificationError> {
    if value.trim().is_empty() {
        return Err(QualificationError::InvalidInput(format!(
            "{field} cannot be empty"
        )));
    }
    Ok(())
}

fn require_optional_non_empty(field: &str, value: Option<&str>) -> Result<(), QualificationError> {
    match value {
        Some(value) => require_non_empty(field, value),
        None => Err(QualificationError::InvalidInput(format!(
            "{field} is required"
        ))),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_supported_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
}

fn midi_event_fingerprint(smf: &midly::Smf<'_>) -> Vec<(usize, u64, u8, String)> {
    let mut fingerprint = Vec::new();
    for (track_index, track) in smf.tracks.iter().enumerate() {
        let mut absolute_tick = 0_u64;
        for event in track {
            absolute_tick += u64::from(event.delta.as_int());
            if let midly::TrackEventKind::Midi { channel, message } = event.kind {
                fingerprint.push((
                    track_index,
                    absolute_tick,
                    channel.as_int(),
                    format!("{message:?}"),
                ));
            }
        }
    }
    fingerprint
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn write_atomic(output_dir: &Path, name: &str, bytes: &[u8]) -> Result<(), HandoffError> {
    let staging = output_dir.join(format!(".{name}.staging"));
    let mut file = fs::File::create(&staging)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(staging, output_dir.join(name))?;
    Ok(())
}
