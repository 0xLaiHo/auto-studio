use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::error::ExperimentError;
use crate::evidence::sha256;
use crate::provider::ProviderUsage;
use crate::runner::{ExperimentRun, RunMode};
use crate::spec::ExperimentalMusicSpec;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FormalSummary {
    pub schema_version: String,
    pub expected_mode_a: usize,
    pub expected_mode_b: usize,
    pub observed_candidates: usize,
    pub completed_candidates: usize,
    pub invalid_candidates: Vec<String>,
    pub mode_b_valid_and_compiled: usize,
    pub mode_b_device_gate_passed: bool,
    pub total_usage: ProviderUsage,
    pub total_latency_ms: u64,
    pub off_peak_cost_usd: f64,
    pub peak_cost_usd: f64,
}

#[derive(Debug, Deserialize)]
struct ProtocolLock {
    provider: FrozenProvider,
    modes: FrozenModes,
    gates: FrozenGates,
    #[serde(default)]
    input_hashes: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct FrozenProvider {
    name: String,
    model_id: String,
    thinking_level: String,
}

#[derive(Debug, Deserialize)]
struct FrozenModes {
    a_brief_ids: Vec<String>,
    b_brief_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FrozenGates {
    mode_b_valid_and_compiled_minimum: String,
}

#[derive(Debug, Deserialize)]
struct Pricing {
    off_peak: PriceBand,
    peak: PriceBand,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct PriceBand {
    input_cache_hit: f64,
    input_cache_miss: f64,
    output: f64,
}

/// Verifies the exact formal candidate plan, all recorded artifact hashes and
/// the frozen Provider identity, then computes the device gate and cost.
///
/// # Errors
///
/// Returns [`ExperimentError`] when the frozen plan and evidence differ, an
/// artifact is corrupt, a compiled result is malformed, or pricing is absent.
pub fn verify_formal(
    assets_root: &Path,
    evidence_root: &Path,
) -> Result<FormalSummary, ExperimentError> {
    let protocol: ProtocolLock = read_json(assets_root.join("protocol.lock.json"))?;
    verify_frozen_inputs(assets_root, &protocol.input_hashes)?;
    let pricing: Pricing = read_json(
        assets_root
            .join("environment")
            .join("deepseek-pricing-v1.json"),
    )?;
    let expected = expected_runs(&protocol.modes);
    verify_exact_run_set(evidence_root, &expected)?;
    let mut runs = Vec::with_capacity(expected.len());
    let mut candidates = HashSet::new();
    for ((mode, brief_id), path) in expected_paths(evidence_root, &expected) {
        let run: ExperimentRun = read_json(path.join("run.json"))?;
        verify_run_identity(&protocol.provider, mode, &brief_id, &run)?;
        if !candidates.insert(run.candidate_id.clone()) {
            return Err(ExperimentError::InvalidInput(format!(
                "duplicate formal candidate identity `{}`",
                run.candidate_id
            )));
        }
        verify_artifacts(&path, &run)?;
        runs.push(run);
    }
    summarize(&protocol, &pricing, &runs)
}

fn verify_frozen_inputs(
    assets_root: &Path,
    input_hashes: &HashMap<String, String>,
) -> Result<(), ExperimentError> {
    for (relative, expected_hash) in input_hashes {
        let path = Path::new(relative);
        if path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(ExperimentError::InvalidInput(format!(
                "unsafe frozen input path `{relative}`"
            )));
        }
        let actual_hash = sha256(&fs::read(assets_root.join(path))?);
        if &actual_hash != expected_hash {
            return Err(ExperimentError::InvalidInput(format!(
                "frozen input hash mismatch: {relative}"
            )));
        }
    }
    Ok(())
}

fn expected_runs(modes: &FrozenModes) -> HashSet<(RunMode, String)> {
    modes
        .a_brief_ids
        .iter()
        .map(|id| (RunMode::A, id.clone()))
        .chain(modes.b_brief_ids.iter().map(|id| (RunMode::B, id.clone())))
        .collect()
}

fn actual_runs(evidence_root: &Path) -> Result<HashSet<(RunMode, String)>, ExperimentError> {
    let mut actual = HashSet::new();
    for (mode, directory) in [(RunMode::A, "mode-a"), (RunMode::B, "mode-b")] {
        let root = evidence_root.join(directory);
        if !root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.join("run.json").is_file() {
                let brief = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        ExperimentError::InvalidInput("non-UTF-8 Brief directory".to_owned())
                    })?;
                actual.insert((mode, brief.to_owned()));
            }
        }
    }
    Ok(actual)
}

fn verify_exact_run_set(
    evidence_root: &Path,
    expected: &HashSet<(RunMode, String)>,
) -> Result<(), ExperimentError> {
    let actual = actual_runs(evidence_root)?;
    if let Some((mode, brief)) = expected.difference(&actual).next() {
        return Err(ExperimentError::InvalidInput(format!(
            "missing formal run mode={} brief={brief}",
            mode.label()
        )));
    }
    if let Some((mode, brief)) = actual.difference(expected).next() {
        return Err(ExperimentError::InvalidInput(format!(
            "unexpected formal run mode={} brief={brief}",
            mode.label()
        )));
    }
    Ok(())
}

fn expected_paths(
    evidence_root: &Path,
    expected: &HashSet<(RunMode, String)>,
) -> HashMap<(RunMode, String), std::path::PathBuf> {
    expected
        .iter()
        .map(|(mode, brief)| {
            let directory = match mode {
                RunMode::A => "mode-a",
                RunMode::B => "mode-b",
                RunMode::C => "mode-c",
            };
            (
                (*mode, brief.clone()),
                evidence_root.join(directory).join(brief),
            )
        })
        .collect()
}

fn verify_run_identity(
    provider: &FrozenProvider,
    expected_mode: RunMode,
    expected_brief: &str,
    run: &ExperimentRun,
) -> Result<(), ExperimentError> {
    if run.mode != expected_mode
        || run.brief_id != expected_brief
        || run.provider != provider.name
        || run.model != provider.model_id
        || run.thinking_level != provider.thinking_level
    {
        return Err(ExperimentError::InvalidInput(format!(
            "formal run identity drift for mode={} brief={expected_brief}",
            expected_mode.label()
        )));
    }
    Ok(())
}

fn verify_artifacts(path: &Path, run: &ExperimentRun) -> Result<(), ExperimentError> {
    for artifact in &run.artifacts {
        if Path::new(&artifact.path).components().count() != 1
            || !matches!(
                Path::new(&artifact.path).components().next(),
                Some(Component::Normal(_))
            )
        {
            return Err(ExperimentError::InvalidInput(format!(
                "unsafe artifact path `{}`",
                artifact.path
            )));
        }
        let bytes = fs::read(path.join(&artifact.path))?;
        if bytes.len() as u64 != artifact.bytes || sha256(&bytes) != artifact.sha256 {
            return Err(ExperimentError::InvalidInput(format!(
                "artifact integrity mismatch: {}",
                path.join(&artifact.path).display()
            )));
        }
    }
    if run.completed_and_compiled() {
        let spec = fs::read_to_string(path.join("spec.json"))?;
        ExperimentalMusicSpec::parse_and_validate(&spec)?;
        midly::Smf::parse(&fs::read(path.join("composition.mid"))?)
            .map_err(|error| ExperimentError::InvalidInput(error.to_string()))?;
    }
    Ok(())
}

fn summarize(
    protocol: &ProtocolLock,
    pricing: &Pricing,
    runs: &[ExperimentRun],
) -> Result<FormalSummary, ExperimentError> {
    let gate_minimum = protocol
        .gates
        .mode_b_valid_and_compiled_minimum
        .split_once('/')
        .and_then(|(minimum, _)| minimum.parse::<usize>().ok())
        .ok_or_else(|| ExperimentError::InvalidInput("invalid Mode B gate".to_owned()))?;
    let mode_b_valid = runs
        .iter()
        .filter(|run| run.mode == RunMode::B && run.completed_and_compiled())
        .count();
    let usage = runs.iter().fold(ProviderUsage::default(), |mut sum, run| {
        sum.add(&run.total_usage);
        sum
    });
    Ok(FormalSummary {
        schema_version: "q0-formal-summary-v1".to_owned(),
        expected_mode_a: protocol.modes.a_brief_ids.len(),
        expected_mode_b: protocol.modes.b_brief_ids.len(),
        observed_candidates: runs.len(),
        completed_candidates: runs
            .iter()
            .filter(|run| run.completed_and_compiled())
            .count(),
        invalid_candidates: runs
            .iter()
            .filter(|run| !run.completed_and_compiled())
            .map(|run| run.candidate_id.clone())
            .collect(),
        mode_b_valid_and_compiled: mode_b_valid,
        mode_b_device_gate_passed: mode_b_valid >= gate_minimum,
        total_latency_ms: runs.iter().map(|run| run.total_latency_ms).sum(),
        off_peak_cost_usd: cost(&usage, pricing.off_peak)?,
        peak_cost_usd: cost(&usage, pricing.peak)?,
        total_usage: usage,
    })
}

fn cost(usage: &ProviderUsage, pricing: PriceBand) -> Result<f64, ExperimentError> {
    let hit = token_count(usage.prompt_cache_hit_tokens)?;
    let miss = token_count(usage.prompt_cache_miss_tokens)?;
    let output = token_count(usage.completion_tokens)?;
    Ok(
        (hit * pricing.input_cache_hit + miss * pricing.input_cache_miss + output * pricing.output)
            / 1_000_000.0,
    )
}

fn token_count(value: Option<u64>) -> Result<f64, ExperimentError> {
    u32::try_from(value.unwrap_or_default())
        .map(f64::from)
        .map_err(|_| ExperimentError::InvalidInput("token count exceeds Q0 bounds".to_owned()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Result<T, ExperimentError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
