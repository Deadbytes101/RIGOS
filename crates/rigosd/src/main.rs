#![forbid(unsafe_code)]

use clap::{Args, Parser, Subcommand};
use rigos_core::{CliEnvelope, Diagnostic, ExecutionStatus};
use rigos_machine::{MACHINE_SCHEMA, MachineContext};
use rigos_miner::MinerBackend;
use rigos_schema::{DOCTOR_SCHEMA, doctor};
use rigos_xmrig::{MINER_SCHEMA, XmrigBackend};
use serde::Serialize;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(name = "rigosd", version = version_text(), about = "DBYTE RIGOS local read-only inspector")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    xmrig_executable: Option<PathBuf>,
    #[arg(long, global = true, value_name = "PATH")]
    xmrig_config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Machine {
        #[command(subcommand)]
        command: InspectCommand,
    },
    Miner {
        #[command(subcommand)]
        command: InspectCommand,
    },
    Doctor(OutputArgs),
}

#[derive(Subcommand)]
enum InspectCommand {
    Inspect(OutputArgs),
}

#[derive(Args, Clone, Copy)]
struct OutputArgs {
    #[arg(long)]
    json: bool,
}

fn version_text() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        " commit=",
        env!("RIGOS_BUILD_COMMIT"),
        " target=",
        env!("RIGOS_BUILD_TARGET"),
        " profile=",
        env!("RIGOS_BUILD_PROFILE")
    )
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = MachineContext::default();
    let backend = XmrigBackend {
        explicit_executable: cli.xmrig_executable,
        explicit_config: cli.xmrig_config,
        probe_version: true,
    };
    match cli.command {
        Command::Machine {
            command: InspectCommand::Inspect(output),
        } => {
            let result = rigos_machine::inspect(&ctx);
            render(
                output.json,
                CliEnvelope::new(
                    "machine.inspect",
                    MACHINE_SCHEMA,
                    result.value,
                    result.diagnostics,
                    result.fatal,
                ),
            )
        }
        Command::Miner {
            command: InspectCommand::Inspect(output),
        } => {
            let result = backend.discover(&ctx);
            render(
                output.json,
                CliEnvelope::new(
                    "miner.inspect",
                    MINER_SCHEMA,
                    result.value,
                    result.diagnostics,
                    result.fatal,
                ),
            )
        }
        Command::Doctor(output) => {
            let machine = rigos_machine::inspect(&ctx);
            let miner = backend.discover(&ctx);
            let data = doctor(&machine.diagnostics, &miner.diagnostics);
            let diagnostics: Vec<Diagnostic> = machine
                .diagnostics
                .into_iter()
                .chain(miner.diagnostics)
                .collect();
            render(
                output.json,
                CliEnvelope::new("doctor", DOCTOR_SCHEMA, Some(data), diagnostics, false),
            )
        }
    }
}

fn render<T: Serialize + std::fmt::Debug>(json: bool, envelope: CliEnvelope<T>) -> ExitCode {
    let status = envelope.status.clone();
    if json {
        match serde_json::to_string_pretty(&envelope) {
            Ok(value) => println!("{value}"),
            Err(_) => return ExitCode::from(4),
        }
    } else {
        println!("{}: {:?}", envelope.command, envelope.status);
        println!("observed at: {}", envelope.observed_at);
        println!("{:#?}", envelope.data);
        for diagnostic in &envelope.diagnostics {
            println!(
                "[{:?}] {}: {}",
                diagnostic.severity, diagnostic.code, diagnostic.message
            );
        }
    }
    match status {
        ExecutionStatus::Error => ExitCode::from(3),
        _ => ExitCode::SUCCESS,
    }
}
