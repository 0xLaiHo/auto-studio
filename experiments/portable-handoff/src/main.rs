use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use autostudio_music_quality::ExperimentalMusicSpec;
use autostudio_portable_handoff::{
    prepare_qualification_matrix, verify_qualification_matrix, write_portable_handoff,
};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "autostudio-portable-handoff")]
#[command(about = "Compile a DAW-neutral symbolic handoff from a Q0 music spec")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write Type-1 MIDI plus a hashed instrument-assignment manifest.
    Compile {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Freeze DAW targets and write a result template bound to the handoff.
    PrepareMatrix {
        #[arg(long)]
        handoff_dir: PathBuf,
        #[arg(long)]
        targets: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Verify DAW evidence and write the qualification summary.
    VerifyMatrix {
        #[arg(long)]
        handoff_dir: PathBuf,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        results: PathBuf,
        #[arg(long)]
        evidence_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("portable handoff failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), autostudio_portable_handoff::HandoffError> {
    match cli.command {
        Command::Compile { input, output_dir } => {
            let input = fs::read_to_string(input)?;
            let spec = ExperimentalMusicSpec::parse_and_validate(&input)?;
            let manifest = write_portable_handoff(&output_dir, &spec)?;
            println!(
                "wrote {} hashed portable artifacts to {}",
                manifest.artifacts.len(),
                output_dir.display()
            );
        }
        Command::PrepareMatrix {
            handoff_dir,
            targets,
            output_dir,
        } => {
            let (plan, _) = prepare_qualification_matrix(&handoff_dir, &targets, &output_dir)?;
            println!(
                "prepared {} DAW qualification targets in {}",
                plan.targets.len(),
                output_dir.display()
            );
        }
        Command::VerifyMatrix {
            handoff_dir,
            plan,
            results,
            evidence_root,
            output,
        } => {
            let summary = verify_qualification_matrix(
                &handoff_dir,
                &plan,
                &results,
                &evidence_root,
                &output,
            )?;
            println!(
                "verified {} DAW targets: {} pass, {} fail, {} not run; MVP gate={}",
                summary.total,
                summary.passed,
                summary.failed,
                summary.not_run,
                summary.all_required_targets_passed
            );
        }
    }
    Ok(())
}
