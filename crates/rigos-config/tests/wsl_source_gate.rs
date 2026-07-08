use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("crates/rigos-config/Cargo.toml").is_file()
        && path.join("scripts/verify.sh").is_file()
}

fn repo_root() -> PathBuf {
    let mut starts = Vec::new();

    if let Ok(current) = env::current_dir() {
        starts.push(current);
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            starts.push(parent.to_path_buf());
        }
    }

    for start in starts {
        let mut candidate = start;
        loop {
            if is_repo_root(&candidate) {
                return candidate;
            }
            if !candidate.pop() {
                break;
            }
        }
    }

    panic!("unable to locate the RIGOS repository root at runtime");
}

fn repo_path(path: &str) -> PathBuf {
    repo_root().join(path)
}

#[test]
fn wsl_launcher_is_path_safe_and_fail_closed() {
    let launcher = fs::read_to_string(repo_path("scripts/verify-wsl.ps1"))
        .expect("read WSL source gate launcher");
    let entrypoint = fs::read_to_string(repo_path("scripts/verify-wsl-entrypoint.sh"))
        .expect("read WSL source gate entrypoint");

    for required in [
        "[string]$Repository,",
        "$PSScriptRoot",
        "RIGOS_WSL_SCRIPT_ROOT_UNAVAILABLE",
        "$Repository.Replace([char]92, [char]47)",
        "wslpath -a -- $RepositoryForWsl",
        "$PathExitCode = $LASTEXITCODE",
        "$ErrorActionPreference = \"Continue\"",
        "verify-wsl-entrypoint.sh",
        "& wsl.exe @WslPrefix -- bash $LinuxEntrypoint $LinuxRepo",
        "RIGOS_WSL_SOURCE_GATE=PASS",
    ] {
        assert!(
            launcher.contains(required),
            "WSL launcher contract missing: {required}"
        );
    }

    for required in [
        "set -euo pipefail",
        "RIGOS_WSL_REPOSITORY_INVALID",
        "for tool in cargo rustc python3 bash sh git grep rg mktemp",
        "command -v \"$tool\"",
        "RIGOS_WSL_TOOL_MISSING",
        "for component in fmt clippy",
        "RIGOS_WSL_CARGO_COMPONENT_MISSING",
        "pycache_root=$(mktemp -d)",
        "trap cleanup EXIT HUP INT TERM",
        "export PYTHONPYCACHEPREFIX=\"$pycache_root\"",
        "export PYTHONDONTWRITEBYTECODE=1",
        "bash ./scripts/verify.sh",
    ] {
        assert!(
            entrypoint.contains(required),
            "WSL entrypoint contract missing: {required}"
        );
    }

    let hard_coded_checkout = ["/mnt/d/TECHNICAL/", "dbyte", "-rigos"].concat();
    let forbidden = [
        "[string]$Repository = (Split-Path -Parent $PSScriptRoot)",
        "wslpath -a $Repository",
        "$PathConverter",
        "$Shell = @'",
        "bash -lc",
        hard_coded_checkout.as_str(),
        "curl | sh",
        "Invoke-WebRequest",
        "rustup-init",
        "exec bash ./scripts/verify.sh",
    ];
    for forbidden in forbidden {
        assert!(
            !launcher.contains(forbidden) && !entrypoint.contains(forbidden),
            "WSL gate contains forbidden bootstrap, multiline shell transport, direct path argument, cleanup-bypassing exec, default expression, or hard-coded path: {forbidden}"
        );
    }
}

#[test]
fn python_bytecode_artifacts_are_ignored() {
    let ignore = fs::read_to_string(repo_path(".gitignore")).expect("read repository ignore rules");
    for required in ["__pycache__/", "*.py[cod]"] {
        assert!(
            ignore.lines().any(|line| line == required),
            "Python bytecode ignore rule missing: {required}"
        );
    }
}

#[test]
fn repository_root_is_resolved_at_runtime() {
    let root = repo_root();
    assert!(is_repo_root(&root));

    let source = fs::read_to_string(root.join("crates/rigos-config/tests/wsl_source_gate.rs"))
        .expect("read WSL source gate test");
    let compile_time_manifest = ["CARGO", "_MANIFEST_DIR"].concat();
    assert!(!source.contains(&compile_time_manifest));
}
