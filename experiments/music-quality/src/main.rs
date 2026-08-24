use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use autostudio_music_quality::{
    DeepSeekClient, ExperimentalMusicSpec, RunMode, default_assets_root, prepare_blind_package,
    resume_mode_b_revision, run_brief, verify_formal, write_compilation_evidence,
};
use clap::{Parser, Subcommand, ValueEnum};

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
    },
    /// Resume only the third revision turn of an interrupted Mode B run.
    ResumeB {
        #[arg(long)]
        brief_id: String,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        assets_root: Option<PathBuf>,
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
        } => {
            let client = DeepSeekClient::from_environment()?;
            let assets_root = assets_root.unwrap_or_else(default_assets_root);
            let run = run_brief(
                &client,
                &assets_root,
                &brief_id,
                mode.into(),
                base_spec.as_deref(),
                &feedback,
                &output_dir,
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
        } => {
            let client = DeepSeekClient::from_environment()?;
            let assets_root = assets_root.unwrap_or_else(default_assets_root);
            let run = resume_mode_b_revision(&client, &assets_root, &brief_id, &output_dir).await?;
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
        } => {
            let assets_root = assets_root.unwrap_or_else(default_assets_root);
            let summary = verify_formal(&assets_root, &evidence_root)?;
            let parent = output.parent().ok_or_else(|| {
                autostudio_music_quality::ExperimentError::InvalidInput(
                    "summary output requires a parent directory".to_owned(),
                )
            })?;
            fs::create_dir_all(parent)?;
            let staging = output.with_extension("json.staging");
            fs::write(&staging, serde_json::to_vec_pretty(&summary)?)?;
            fs::rename(staging, &output)?;
            println!(
                "verified {} candidates; Mode B device gate={} peak_cost_usd={:.6}",
                summary.observed_candidates,
                summary.mode_b_device_gate_passed,
                summary.peak_cost_usd
            );
        }
    }
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
