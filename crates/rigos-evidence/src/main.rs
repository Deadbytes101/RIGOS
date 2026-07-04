use clap::{Parser, Subcommand};
use rigos_schema::{BUILD_MANIFEST_SCHEMA, BuildManifestV1};
use std::{collections::BTreeMap, fs, path::PathBuf, process::ExitCode};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Recipients {
        #[arg(long)]
        file: PathBuf,
    },
    Sanitize {
        #[arg(long)]
        raw: PathBuf,
        #[arg(long)]
        public: PathBuf,
        #[arg(long)]
        node_alias: String,
    },
    Sha256 {
        path: PathBuf,
    },
    BuildManifest {
        #[arg(long)]
        rc: String,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        schemas: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        rustc: String,
        #[arg(long)]
        cargo: String,
        #[arg(long)]
        build_os: String,
        #[arg(long)]
        kernel: String,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Recipients { file } => rigos_evidence::load_recipients(&file)
            .and_then(|v| serde_json::to_string_pretty(&v).map_err(Into::into)),
        Command::Sanitize {
            raw,
            public,
            node_alias,
        } => rigos_evidence::sanitize_approved(&raw, &public, &node_alias)
            .and_then(|v| serde_json::to_string_pretty(&v).map_err(Into::into)),
        Command::Sha256 { path } => rigos_evidence::sha256_file(&path),
        Command::BuildManifest {
            rc,
            commit,
            binary,
            schemas,
            output,
            rustc,
            cargo,
            build_os,
            kernel,
        } => build_manifest(
            rc, commit, binary, schemas, output, rustc, cargo, build_os, kernel,
        ),
    };
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_manifest(
    rc: String,
    commit: String,
    binary: PathBuf,
    schemas: PathBuf,
    output: PathBuf,
    rustc: String,
    cargo: String,
    build_os: String,
    kernel: String,
) -> Result<String, rigos_evidence::EvidenceError> {
    let mut schema_hashes = BTreeMap::new();
    for entry in fs::read_dir(schemas)? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) == Some("json") {
            schema_hashes.insert(
                path.file_name().unwrap().to_string_lossy().into_owned(),
                rigos_evidence::sha256_file(&path)?,
            );
        }
    }
    let manifest = BuildManifestV1 {
        schema: BUILD_MANIFEST_SCHEMA.into(),
        artifact: "rigosd".into(),
        release_candidate: rc,
        git_commit: commit,
        git_tree_clean: true,
        target: "x86_64-unknown-linux-gnu".into(),
        build_os,
        kernel,
        rustc,
        cargo,
        build_profile: "release".into(),
        binary_sha256: rigos_evidence::sha256_file(&binary)?,
        schemas_sha256: schema_hashes,
    };
    let rendered = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    fs::write(output, &rendered)?;
    Ok(rendered)
}
