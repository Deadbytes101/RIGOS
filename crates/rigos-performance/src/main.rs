#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use rigos_performance::{AuthorityPaths, ProcKernel, SystemdMinerControl, execute};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(about = "RIGOS machine performance authority")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(long, default_value = "/var/lib/rigos", hide = true)]
    state_root: PathBuf,
    #[arg(
        long,
        default_value = "/run/rigos/performance-status.json",
        hide = true
    )]
    status: PathBuf,
    #[arg(long, default_value = "/proc/sys/kernel/random/boot_id", hide = true)]
    boot_id: PathBuf,
    #[arg(long, default_value = "/proc/meminfo", hide = true)]
    meminfo: PathBuf,
    #[arg(long, default_value = "/proc/sys/vm/nr_hugepages", hide = true)]
    nr_hugepages: PathBuf,
    #[arg(long, default_value = "/usr/bin/systemctl", hide = true)]
    systemctl: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    Hugepages {
        #[command(subcommand)]
        command: HugepagesCommand,
    },
}

#[derive(Subcommand)]
enum HugepagesCommand {
    Apply,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Hugepages {
            command: HugepagesCommand::Apply,
        } => {
            let paths = AuthorityPaths {
                state_root: cli.state_root,
                status: cli.status,
                boot_id: cli.boot_id,
            };
            let mut kernel = ProcKernel {
                meminfo: cli.meminfo,
                nr_hugepages: cli.nr_hugepages,
            };
            let mut miner = SystemdMinerControl {
                executable: cli.systemctl,
            };
            match execute(&paths, &mut kernel, &mut miner) {
                Ok(status) => {
                    println!(
                        "{}",
                        serde_json::to_string(&status).expect("serialize performance status")
                    );
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("rigos-performance: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
