use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::compile_to_smf;
use crate::constants::{
    BLIND_SEED, BRIEF_FILE, CORPUS_PATH, FULL_SPEC_MAX_TOKENS, MIDI_FILE, MODE_A_PROMPT_PATH,
    MODE_B_ARRANGE_PROMPT_PATH, MODE_B_REVISE_PROMPT_PATH, MODE_B_SKELETON_PROMPT_PATH,
    MODE_C_FEEDBACK_PROMPT_PATH, NORMALIZED_SPEC_FILE, RUN_RECORD_FILE, RUN_SCHEMA_VERSION,
    SCHEMA_PATH, SKELETON_MAX_TOKENS, SYSTEM_PROMPT_PATH, VALIDATION_ERROR_FILE,
};
use crate::error::ExperimentError;
use crate::evidence::{
    ArtifactRecord, record_existing_artifact, write_atomic, write_hashed_artifact,
};
use crate::provider::{DeepSeekClient, ProviderTurn, ProviderUsage};
use crate::spec::ExperimentalMusicSpec;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Corpus {
    pub schema_version: String,
    pub frozen_at: String,
    pub briefs: Vec<FrozenBrief>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FrozenBrief {
    pub id: String,
    pub level: String,
    pub title: String,
    pub purpose: String,
    pub duration_seconds: u32,
    pub section_plan: Vec<String>,
    pub style: Vec<String>,
    pub mood: Vec<String>,
    pub must_include: Vec<String>,
    pub must_avoid: Vec<String>,
    pub instrument_roles: Vec<String>,
    pub delivery: Vec<String>,
    pub score_focus: Vec<String>,
    pub mode_a: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    A,
    B,
    C,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExperimentRun {
    pub schema_version: String,
    pub run_id: String,
    pub candidate_id: String,
    pub status: String,
    pub mode: RunMode,
    pub brief_id: String,
    pub brief_level: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub thinking_level: String,
    pub schema_valid: bool,
    pub compiled: bool,
    pub validation_error: Option<String>,
    pub turn_count: usize,
    pub total_usage: ProviderUsage,
    pub total_latency_ms: u64,
    pub artifacts: Vec<ArtifactRecord>,
}

impl ExperimentRun {
    #[must_use]
    pub fn completed_and_compiled(&self) -> bool {
        self.status == "completed" && self.schema_valid && self.compiled
    }
}

struct GenerationInputs<'a> {
    assets_root: &'a Path,
    system: &'a str,
    schema: &'a str,
    brief_json: &'a str,
}

struct FinalizedArtifacts {
    schema_valid: bool,
    compiled: bool,
    validation_error: Option<String>,
    artifacts: Vec<ArtifactRecord>,
}

struct GeneratedTurns {
    turns: Vec<ProviderTurn>,
    artifacts: Vec<ArtifactRecord>,
}

/// Loads the frozen Q0 corpus from an experiment asset root.
///
/// # Errors
///
/// Returns [`ExperimentError`] when the file is missing or invalid.
pub fn load_corpus(assets_root: &Path) -> Result<Corpus, ExperimentError> {
    let bytes = fs::read(assets_root.join(CORPUS_PATH))?;
    let corpus: Corpus = serde_json::from_slice(&bytes)?;
    if corpus.briefs.len() != 12 {
        return Err(ExperimentError::InvalidInput(format!(
            "frozen corpus must contain exactly 12 briefs, found {}",
            corpus.briefs.len()
        )));
    }
    Ok(corpus)
}

/// Runs one frozen Brief through Mode A, B or C and writes an auditable bundle.
///
/// Mode C requires a base spec and one or two non-empty Creator feedback
/// strings. Invalid Provider output is recorded as an `invalid` run rather
/// than silently repaired.
///
/// # Errors
///
/// Returns [`ExperimentError`] for missing assets/Briefs, invalid Mode C input,
/// Provider failure or evidence write failure.
pub async fn run_brief(
    client: &DeepSeekClient,
    assets_root: &Path,
    brief_id: &str,
    mode: RunMode,
    base_spec: Option<&Path>,
    feedback: &[String],
    output_dir: &Path,
) -> Result<ExperimentRun, ExperimentError> {
    let corpus = load_corpus(assets_root)?;
    let brief = corpus
        .briefs
        .iter()
        .find(|brief| brief.id == brief_id)
        .ok_or_else(|| ExperimentError::InvalidInput(format!("unknown brief `{brief_id}`")))?;
    validate_mode_input(mode, base_spec, feedback)?;
    fs::create_dir_all(output_dir)?;
    let started_at = Utc::now();
    let system = fs::read_to_string(assets_root.join(SYSTEM_PROMPT_PATH))?;
    let schema = fs::read_to_string(assets_root.join(SCHEMA_PATH))?;
    let brief_json = serde_json::to_string_pretty(brief)?;
    let mut artifacts = vec![write_hashed_artifact(
        output_dir,
        BRIEF_FILE,
        brief_json.as_bytes(),
    )?];
    let inputs = GenerationInputs {
        assets_root,
        system: &system,
        schema: &schema,
        brief_json: &brief_json,
    };
    let generated = generate_turns(client, mode, &inputs, base_spec, feedback, output_dir).await?;
    artifacts.extend(generated.artifacts);
    let turns = generated.turns;
    let finalized = finalize_artifacts(output_dir, &turns, artifacts)?;
    let run = assemble_run(brief, mode, started_at, &turns, finalized)?;
    write_atomic(
        output_dir,
        RUN_RECORD_FILE,
        &serde_json::to_vec_pretty(&run)?,
    )?;
    Ok(run)
}

fn assemble_run(
    brief: &FrozenBrief,
    mode: RunMode,
    started_at: DateTime<Utc>,
    turns: &[ProviderTurn],
    finalized: FinalizedArtifacts,
) -> Result<ExperimentRun, ExperimentError> {
    let last = turns
        .last()
        .ok_or_else(|| ExperimentError::InvalidInput("run produced no Provider turn".to_owned()))?;
    Ok(ExperimentRun {
        schema_version: RUN_SCHEMA_VERSION.to_owned(),
        run_id: format!(
            "{}-{}-{}",
            brief.id,
            mode.label(),
            started_at.timestamp_millis()
        ),
        candidate_id: candidate_id(&brief.id, mode),
        status: if finalized.compiled {
            "completed"
        } else {
            "invalid"
        }
        .to_owned(),
        mode,
        brief_id: brief.id.clone(),
        brief_level: brief.level.clone(),
        started_at,
        completed_at: Utc::now(),
        provider: last.provider.clone(),
        model: last.model.clone(),
        thinking_level: last.thinking_level.clone(),
        schema_valid: finalized.schema_valid,
        compiled: finalized.compiled,
        validation_error: finalized.validation_error,
        turn_count: turns.len(),
        total_usage: aggregate_usage(turns),
        total_latency_ms: turns.iter().map(|turn| turn.latency_ms).sum(),
        artifacts: finalized.artifacts,
    })
}

/// Resumes a Mode B run after two normalized turns were durably persisted.
///
/// # Errors
///
/// Returns [`ExperimentError`] when prior turns are missing/malformed, the
/// frozen Brief is unknown, the revision call fails, or evidence cannot be
/// finalized.
pub async fn resume_mode_b_revision(
    client: &DeepSeekClient,
    assets_root: &Path,
    brief_id: &str,
    output_dir: &Path,
) -> Result<ExperimentRun, ExperimentError> {
    let corpus = load_corpus(assets_root)?;
    let brief = corpus
        .briefs
        .iter()
        .find(|brief| brief.id == brief_id)
        .ok_or_else(|| ExperimentError::InvalidInput(format!("unknown brief `{brief_id}`")))?;
    let started_at = Utc::now();
    let system = fs::read_to_string(assets_root.join(SYSTEM_PROMPT_PATH))?;
    let schema = fs::read_to_string(assets_root.join(SCHEMA_PATH))?;
    let brief_json = serde_json::to_string_pretty(brief)?;
    let inputs = GenerationInputs {
        assets_root,
        system: &system,
        schema: &schema,
        brief_json: &brief_json,
    };
    let skeleton = read_turn(output_dir, 1)?;
    let arrangement = read_turn(output_dir, 2)?;
    let revision = generate_mode_b_revision(client, &inputs, &skeleton, &arrangement).await?;
    let revision_artifact = persist_turn(output_dir, 3, &revision)?;
    let turns = vec![skeleton, arrangement, revision];
    let artifacts = vec![
        write_hashed_artifact(output_dir, BRIEF_FILE, brief_json.as_bytes())?,
        record_existing_artifact(output_dir, "turn-01.json")?,
        record_existing_artifact(output_dir, "turn-02.json")?,
        revision_artifact,
    ];
    let finalized = finalize_artifacts(output_dir, &turns, artifacts)?;
    let run = assemble_run(brief, RunMode::B, started_at, &turns, finalized)?;
    write_atomic(
        output_dir,
        RUN_RECORD_FILE,
        &serde_json::to_vec_pretty(&run)?,
    )?;
    Ok(run)
}

async fn generate_turns(
    client: &DeepSeekClient,
    mode: RunMode,
    inputs: &GenerationInputs<'_>,
    base_spec: Option<&Path>,
    feedback: &[String],
    output_dir: &Path,
) -> Result<GeneratedTurns, ExperimentError> {
    match mode {
        RunMode::A => {
            let instruction = fs::read_to_string(inputs.assets_root.join(MODE_A_PROMPT_PATH))?;
            let user = format!(
                "{instruction}\n\nFROZEN BRIEF:\n{}\n\nEXPERIMENTAL MUSIC SPEC SCHEMA:\n{}",
                inputs.brief_json, inputs.schema
            );
            let turn = client
                .generate_json(inputs.system, &user, FULL_SPEC_MAX_TOKENS)
                .await?;
            let artifact = persist_turn(output_dir, 1, &turn)?;
            Ok(GeneratedTurns {
                turns: vec![turn],
                artifacts: vec![artifact],
            })
        }
        RunMode::B => run_mode_b(client, inputs, output_dir).await,
        RunMode::C => {
            let base_spec = base_spec.ok_or_else(|| {
                ExperimentError::InvalidInput("Mode C requires a base spec".to_owned())
            })?;
            run_mode_c(client, inputs, base_spec, feedback, output_dir).await
        }
    }
}

fn finalize_artifacts(
    output_dir: &Path,
    turns: &[ProviderTurn],
    mut artifacts: Vec<ArtifactRecord>,
) -> Result<FinalizedArtifacts, ExperimentError> {
    let final_content = turns
        .last()
        .map(|turn| turn.content.trim())
        .ok_or_else(|| ExperimentError::InvalidInput("run produced no Provider turn".to_owned()))?;
    match ExperimentalMusicSpec::parse_and_validate(final_content) {
        Ok(spec) => {
            artifacts.push(write_hashed_artifact(
                output_dir,
                NORMALIZED_SPEC_FILE,
                &serde_json::to_vec_pretty(&spec)?,
            )?);
            artifacts.push(write_hashed_artifact(
                output_dir,
                MIDI_FILE,
                &compile_to_smf(&spec)?,
            )?);
            Ok(FinalizedArtifacts {
                schema_valid: true,
                compiled: true,
                validation_error: None,
                artifacts,
            })
        }
        Err(error) => {
            let message = error.to_string();
            artifacts.push(write_hashed_artifact(
                output_dir,
                VALIDATION_ERROR_FILE,
                message.as_bytes(),
            )?);
            Ok(FinalizedArtifacts {
                schema_valid: false,
                compiled: false,
                validation_error: Some(message),
                artifacts,
            })
        }
    }
}

async fn run_mode_b(
    client: &DeepSeekClient,
    inputs: &GenerationInputs<'_>,
    output_dir: &Path,
) -> Result<GeneratedTurns, ExperimentError> {
    let mut artifacts = Vec::with_capacity(3);
    let skeleton_instruction =
        fs::read_to_string(inputs.assets_root.join(MODE_B_SKELETON_PROMPT_PATH))?;
    let skeleton_user = format!(
        "{skeleton_instruction}\n\nFROZEN BRIEF:\n{}",
        inputs.brief_json
    );
    let skeleton = client
        .generate_json(inputs.system, &skeleton_user, SKELETON_MAX_TOKENS)
        .await?;
    artifacts.push(persist_turn(output_dir, 1, &skeleton)?);
    let arrange_instruction =
        fs::read_to_string(inputs.assets_root.join(MODE_B_ARRANGE_PROMPT_PATH))?;
    let arrange_user = format!(
        "{arrange_instruction}\n\nFROZEN BRIEF:\n{}\n\nSKELETON:\n{}\n\nEXPERIMENTAL MUSIC SPEC SCHEMA:\n{}",
        inputs.brief_json, skeleton.content, inputs.schema
    );
    let arrangement = client
        .generate_json(inputs.system, &arrange_user, FULL_SPEC_MAX_TOKENS)
        .await?;
    artifacts.push(persist_turn(output_dir, 2, &arrangement)?);
    let revised = generate_mode_b_revision(client, inputs, &skeleton, &arrangement).await?;
    artifacts.push(persist_turn(output_dir, 3, &revised)?);
    Ok(GeneratedTurns {
        turns: vec![skeleton, arrangement, revised],
        artifacts,
    })
}

async fn generate_mode_b_revision(
    client: &DeepSeekClient,
    inputs: &GenerationInputs<'_>,
    skeleton: &ProviderTurn,
    arrangement: &ProviderTurn,
) -> Result<ProviderTurn, ExperimentError> {
    let diagnostics = match ExperimentalMusicSpec::parse_and_validate(arrangement.content.trim()) {
        Ok(spec) => {
            let musical = spec.violations();
            if musical.is_empty() {
                "schema and structural validator passed; still improve musical decisions where needed"
                    .to_owned()
            } else {
                musical.join("; ")
            }
        }
        Err(error) => error.to_string(),
    };
    let revise_instruction =
        fs::read_to_string(inputs.assets_root.join(MODE_B_REVISE_PROMPT_PATH))?;
    let revise_user = format!(
        "{revise_instruction}\n\nFROZEN BRIEF:\n{}\n\nSKELETON:\n{}\n\nCURRENT SPEC:\n{}\n\nVALIDATOR DIAGNOSTICS:\n{diagnostics}\n\nEXPERIMENTAL MUSIC SPEC SCHEMA:\n{}",
        inputs.brief_json, skeleton.content, arrangement.content, inputs.schema
    );
    let revised = client
        .generate_json(inputs.system, &revise_user, FULL_SPEC_MAX_TOKENS)
        .await?;
    Ok(revised)
}

async fn run_mode_c(
    client: &DeepSeekClient,
    inputs: &GenerationInputs<'_>,
    base_spec: &Path,
    feedback: &[String],
    output_dir: &Path,
) -> Result<GeneratedTurns, ExperimentError> {
    let instruction = fs::read_to_string(inputs.assets_root.join(MODE_C_FEEDBACK_PROMPT_PATH))?;
    let mut current = fs::read_to_string(base_spec)?;
    ExperimentalMusicSpec::parse_and_validate(&current)?;
    let mut turns = Vec::new();
    let mut artifacts = Vec::new();
    for creator_feedback in feedback {
        let user = format!(
            "{instruction}\n\nFROZEN BRIEF:\n{}\n\nCURRENT SPEC:\n{current}\n\nCREATOR FEEDBACK:\n{creator_feedback}\n\nEXPERIMENTAL MUSIC SPEC SCHEMA:\n{}",
            inputs.brief_json, inputs.schema
        );
        let turn = client
            .generate_json(inputs.system, &user, FULL_SPEC_MAX_TOKENS)
            .await?;
        current.clone_from(&turn.content);
        artifacts.push(persist_turn(output_dir, turns.len() + 1, &turn)?);
        turns.push(turn);
    }
    Ok(GeneratedTurns { turns, artifacts })
}

fn persist_turn(
    output_dir: &Path,
    index: usize,
    turn: &ProviderTurn,
) -> Result<ArtifactRecord, ExperimentError> {
    let name = format!("turn-{index:02}.json");
    write_hashed_artifact(output_dir, &name, &serde_json::to_vec_pretty(turn)?)
}

fn read_turn(output_dir: &Path, index: usize) -> Result<ProviderTurn, ExperimentError> {
    let name = format!("turn-{index:02}.json");
    Ok(serde_json::from_slice(&fs::read(output_dir.join(name))?)?)
}

fn validate_mode_input(
    mode: RunMode,
    base_spec: Option<&Path>,
    feedback: &[String],
) -> Result<(), ExperimentError> {
    if mode == RunMode::C {
        if base_spec.is_none() {
            return Err(ExperimentError::InvalidInput(
                "Mode C requires --base-spec".to_owned(),
            ));
        }
        if feedback.is_empty()
            || feedback.len() > 2
            || feedback.iter().any(|item| item.trim().is_empty())
        {
            return Err(ExperimentError::InvalidInput(
                "Mode C requires one or two non-empty Creator feedback entries".to_owned(),
            ));
        }
    } else if base_spec.is_some() || !feedback.is_empty() {
        return Err(ExperimentError::InvalidInput(
            "base spec and feedback are valid only for Mode C".to_owned(),
        ));
    }
    Ok(())
}

fn aggregate_usage(turns: &[ProviderTurn]) -> ProviderUsage {
    ProviderUsage {
        prompt_tokens: checked_sum(turns.iter().map(|turn| turn.usage.prompt_tokens)),
        prompt_cache_hit_tokens: checked_sum(
            turns.iter().map(|turn| turn.usage.prompt_cache_hit_tokens),
        ),
        prompt_cache_miss_tokens: checked_sum(
            turns.iter().map(|turn| turn.usage.prompt_cache_miss_tokens),
        ),
        completion_tokens: checked_sum(turns.iter().map(|turn| turn.usage.completion_tokens)),
        total_tokens: checked_sum(turns.iter().map(|turn| turn.usage.total_tokens)),
    }
}

fn checked_sum(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    let mut total = 0_u64;
    for value in values {
        total = total.checked_add(value?)?;
    }
    Some(total)
}

fn candidate_id(brief_id: &str, mode: RunMode) -> String {
    let digest = hex::encode(Sha256::digest(format!(
        "{BLIND_SEED}|{brief_id}|{}",
        mode.label()
    )));
    format!("c-{}", &digest[..12])
}

impl RunMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
            Self::C => "c",
        }
    }
}

#[must_use]
pub fn default_assets_root() -> PathBuf {
    PathBuf::from(crate::constants::DEFAULT_ASSETS_ROOT)
}
