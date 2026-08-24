use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use autostudio_music_quality::{
    DeepSeekClient, ExperimentalMusicSpec, RunMode, RunPolicy, default_assets_root,
    prepare_blind_package, resume_mode_b_with_policy, run_brief_with_policy, verify_formal,
    verify_formal_with_protocol, write_compilation_evidence,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(name = "autostudio-music-quality")]
#[command(about = "Reproducible Q0 music-content feasibility experiment")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a spec and compile a hashed MIDI evidence bundle.
    Compile {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Run one frozen Brief through the real `DeepSeek` Provider.
    Run {
        #[arg(long, value_enum)]
        mode: CliRunMode,
        #[arg(long)]
        brief_id: String,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        base_spec: Option<PathBuf>,
        #[arg(long = "feedback")]
        feedback: Vec<String>,
        #[arg(long)]
        assets_root: Option<PathBuf>,
        /// Immutable protocol lock that binds this run and enables its policies.
        #[arg(long)]
        protocol_lock: Option<PathBuf>,
    },
    /// Resume only the third revision turn of an interrupted Mode B run.
    ResumeB {
        #[arg(long)]
        brief_id: String,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        assets_root: Option<PathBuf>,
        /// Immutable protocol lock used for the interrupted run.
        #[arg(long)]
        protocol_lock: Option<PathBuf>,
    },
    /// Build an evaluator-safe package and a separate private mode mapping.
    PrepareBlind {
        #[arg(long)]
        evidence_root: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Verify the exact locked A/B set and write its aggregate summary.
    VerifyFormal {
        #[arg(long)]
        evidence_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        assets_root: Option<PathBuf>,
        /// Explicit protocol lock; defaults to `protocol.lock.json` (v2).
        #[arg(long)]
        protocol_lock: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliRunMode {
    A,
    B,
    C,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Q0 experiment failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), autostudio_music_quality::ExperimentError> {
    match cli.command {
        Command::Compile { input, output_dir } => {
            let input = fs::read_to_string(input)?;
            let spec = ExperimentalMusicSpec::parse_and_validate(&input)?;
            let manifest = write_compilation_evidence(&output_dir, &spec)?;
            println!(
                "wrote {} hashed artifacts to {}",
                manifest.artifacts.len(),
                output_dir.display()
            );
        }
        Command::Run {
            mode,
            brief_id,
            output_dir,
            base_spec,
            feedback,
            assets_root,
            protocol_lock,
        } => {
            let client = DeepSeekClient::from_environment()?;
            let assets_root = assets_root.unwrap_or_else(default_assets_root);
            let mode = RunMode::from(mode);
            let policy = protocol_lock.as_deref().map_or_else(
                || Ok(RunPolicy::default()),
                |path| load_run_policy(path, mode, &brief_id),
            )?;
            let run = run_brief_with_policy(
                &client,
                &assets_root,
                &brief_id,
                mode,
                base_spec.as_deref(),
                &feedback,
                &output_dir,
                &policy,
            )
            .await?;
            println!(
                "{} {} -> {} ({}, {} turns)",
                run.mode.label(),
                run.brief_id,
                run.status,
                run.candidate_id,
                run.turn_count
            );
        }
        Command::ResumeB {
            brief_id,
            output_dir,
            assets_root,
            protocol_lock,
        } => {
            let client = DeepSeekClient::from_environment()?;
            let assets_root = assets_root.unwrap_or_else(default_assets_root);
            let policy = protocol_lock.as_deref().map_or_else(
                || Ok(RunPolicy::default()),
                |path| load_run_policy(path, RunMode::B, &brief_id),
            )?;
            let run =
                resume_mode_b_with_policy(&client, &assets_root, &brief_id, &output_dir, &policy)
                    .await?;
            println!(
                "resumed b {} -> {} ({}, {} turns)",
                run.brief_id, run.status, run.candidate_id, run.turn_count
            );
        }
        Command::PrepareBlind {
            evidence_root,
            output_dir,
        } => {
            let manifest = prepare_blind_package(&evidence_root, &output_dir)?;
            println!(
                "prepared {} blind candidates in {}",
                manifest.candidates.len(),
                output_dir.display()
            );
        }
        Command::VerifyFormal {
            evidence_root,
            output,
            assets_root,
            protocol_lock,
        } => verify_and_write_formal_summary(
            &evidence_root,
            &output,
            assets_root.as_deref(),
            protocol_lock.as_deref(),
        )?,
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ExecutionProtocolLock {
    schema_version: String,
    run_binding_required: bool,
    modes: ExecutionModes,
    #[serde(default)]
    mode_b_resource_repair: Option<ExecutionRepairPolicy>,
}

#[derive(Debug, Deserialize)]
struct ExecutionModes {
    #[serde(rename = "a_brief_ids")]
    a: Vec<String>,
    #[serde(rename = "b_brief_ids")]
    b: Vec<String>,
    #[serde(rename = "c_brief_ids")]
    c: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExecutionRepairPolicy {
    max_turns: u8,
}

fn load_run_policy(
    path: &std::path::Path,
    mode: RunMode,
    brief_id: &str,
) -> Result<RunPolicy, autostudio_music_quality::ExperimentError> {
    let bytes = fs::read(path)?;
    let protocol: ExecutionProtocolLock = serde_json::from_slice(&bytes)?;
    if !protocol.run_binding_required {
        return Err(autostudio_music_quality::ExperimentError::InvalidInput(
            "explicit run protocol must require per-run binding".to_owned(),
        ));
    }
    let expected = match mode {
        RunMode::A => &protocol.modes.a,
        RunMode::B => &protocol.modes.b,
        RunMode::C => &protocol.modes.c,
    };
    if !expected.iter().any(|candidate| candidate == brief_id) {
        return Err(autostudio_music_quality::ExperimentError::InvalidInput(
            format!(
                "protocol `{}` does not authorize mode={} brief={brief_id}",
                protocol.schema_version,
                mode.label()
            ),
        ));
    }
    let repair_max = protocol
        .mode_b_resource_repair
        .map_or(0, |repair| repair.max_turns);
    RunPolicy::locked(
        protocol.schema_version,
        hex::encode(Sha256::digest(bytes)),
        repair_max,
    )
}

fn verify_and_write_formal_summary(
    evidence_root: &std::path::Path,
    output: &std::path::Path,
    assets_root: Option<&std::path::Path>,
    protocol_lock: Option<&std::path::Path>,
) -> Result<(), autostudio_music_quality::ExperimentError> {
    let default_root = default_assets_root();
    let assets_root = assets_root.unwrap_or(&default_root);
    let summary = if let Some(protocol_lock) = protocol_lock {
        verify_formal_with_protocol(assets_root, evidence_root, protocol_lock)?
    } else {
        verify_formal(assets_root, evidence_root)?
    };
    let parent = output.parent().ok_or_else(|| {
        autostudio_music_quality::ExperimentError::InvalidInput(
            "summary output requires a parent directory".to_owned(),
        )
    })?;
    fs::create_dir_all(parent)?;
    let staging = output.with_extension("json.staging");
    fs::write(&staging, serde_json::to_vec_pretty(&summary)?)?;
    fs::rename(staging, output)?;
    println!(
        "verified {} candidates; Mode B device gate={} peak_cost_usd={:.6}",
        summary.observed_candidates, summary.mode_b_device_gate_passed, summary.peak_cost_usd
    );
    Ok(())
}

impl From<CliRunMode> for RunMode {
    fn from(value: CliRunMode) -> Self {
        match value {
            CliRunMode::A => Self::A,
            CliRunMode::B => Self::B,
            CliRunMode::C => Self::C,
        }
    }
}
