use std::{fs, path::PathBuf, process::Command};

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn repo_file(relative: &str) -> String {
    fs::read_to_string(repo_path(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

#[test]
fn short_rig_operator_command_is_thin_safe_and_observable() {
    let rig = repo_file("build/usb/includes.chroot/usr/local/bin/rig");
    let hook = repo_file("build/usb/hooks/010-rigos.chroot");

    for command in [
        "status",
        "health",
        "start",
        "stop",
        "restart",
        "logs",
        "config",
        "firstboot",
        "recover",
        "version",
        "help",
    ] {
        assert!(
            rig.contains(command),
            "short rig command is missing operator surface: {command}"
        );
    }

    for alias in ["s", "h", "up", "down", "r", "log"] {
        assert!(rig.contains(alias), "short rig alias is missing: {alias}");
    }

    for authority in [
        "rigosctl health inspect --json",
        "rigosctl state inspect --json",
        "journalctl",
        "systemctl",
        "rigos-miner.service",
    ] {
        assert!(
            rig.contains(authority),
            "rig must delegate to existing authority: {authority}"
        );
    }

    for required in [
        "OPERATOR_STATUS_SCHEMA = \"rigos.operator-status/v1\"",
        "OPERATOR_HEALTH_SCHEMA = \"rigos.operator-health/v1\"",
        "def load_operator_snapshot()",
        "def load_operator_health()",
        "def require_root(action: str)",
        "def wait_miner_active(",
        "def print_gate_reason()",
        "firstboot refused: run from local tty1",
        "rollback is not available in this alpha image",
    ] {
        assert!(
            rig.contains(required),
            "rig operator contract is missing: {required}"
        );
    }

    for forbidden in [
        "\"sudo\"",
        "'sudo'",
        "shell=True",
        "eval(",
        "BEGIN OPENSSH PRIVATE KEY",
        "access-token",
        "xmrig-api-token",
        "wallet",
        "password=",
    ] {
        assert!(
            !rig.contains(forbidden),
            "rig must not contain unsafe operator surface marker: {forbidden}"
        );
    }

    assert!(
        hook.contains("chmod 0755 /usr/local/bin/rig "),
        "image hook must install rig as an appliance executable"
    );
}

#[test]
fn short_rig_json_is_allowlisted_and_does_not_forward_future_secret_fields() {
    let fixture = r#"
import contextlib
import io
import json
import runpy
import subprocess
import sys
from pathlib import Path

source = Path(sys.argv[1])
namespace = runpy.run_path(str(source), run_name="rig_operator_test")
g = namespace["main"].__globals__

class Result:
    def __init__(self, returncode=0, stdout="", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr

def envelope(command, data):
    return {
        "schema": "rigos.cli-envelope/v1",
        "command": command,
        "status": "ok",
        "observed_at": "2026-07-12T00:00:00Z",
        "data": data,
        "meta": {"image_version": "0.0.4-alpha.26"},
    }

def fake_run(argv, **_kwargs):
    text = " ".join(argv)
    if text.endswith("health inspect --json"):
        return Result(stdout=json.dumps(envelope("health.inspect", {
            "miner": {
                "state": "ready",
                "future_secret": "do-not-forward",
                "api": {"metrics": {
                    "algorithm": "rx/0",
                    "pool": "pool.example:10001",
                    "hashrate_10s": 123.4,
                    "accepted_shares": 9,
                    "rejected_shares": 0,
                    "hugepages_used": 1168,
                    "hugepages_total": 1168,
                    "identity": "secret-identity",
                }},
                "config": {"current_revision": "rev-a", "private_sha256": "secret-hash"},
                "remediation": {"action": "none"},
            },
            "runtime": {
                "algorithm": "rx/0",
                "private_sha256": "secret-hash",
                "http_api": {"token_path": "/run/rigos/token"},
            },
            "network": {"state": "ready"},
            "activation": {"revision": "rev-a"},
            "state": {"outcome": "ready"},
        })))
    if text.endswith("state inspect --json"):
        return Result(stdout=json.dumps(envelope("state.inspect", {
            "current_revision": "rev-a",
            "state_status": {"outcome": "ready"},
            "activation_status": {"outcome": "ready"},
            "runtime_config_status": {"outcome": "ready", "private_sha256": "secret-hash"},
            "unknown_future": "do-not-forward",
        })))
    if text.endswith("network inspect --json"):
        return Result(stdout=json.dumps(envelope("network.inspect", {"state": "ready"})))
    return Result(returncode=1, stderr="unexpected command: " + text)

g["subprocess"].run = fake_run
snapshot = namespace["load_operator_snapshot"]()
encoded = json.dumps(snapshot, sort_keys=True)
for forbidden in [
    "do-not-forward",
    "secret-hash",
    "/run/rigos/token",
    "secret-identity",
    "private_sha256",
    "token_path",
    "identity",
]:
    assert forbidden not in encoded, forbidden
assert snapshot["schema"] == "rigos.operator-status/v1"
assert snapshot["health"] == "ready"
assert snapshot["configuration_revision"] == "rev-a"

buffer = io.StringIO()
with contextlib.redirect_stdout(buffer):
    assert namespace["print_health"](True) == 0
health = buffer.getvalue()
assert "rigos.operator-health/v1" in health
assert "secret" not in health and "token" not in health

g["is_root"] = lambda: False
try:
    namespace["miner_start"]("start")
    raise AssertionError("miner_start did not reject unprivileged execution")
except namespace["RigError"] as error:
    assert error.code == 4
    assert "requires root privileges" in str(error)
"#;

    let result = Command::new("python3")
        .arg("-c")
        .arg(fixture)
        .arg(repo_path("build/usb/includes.chroot/usr/local/bin/rig"))
        .status()
        .expect("run rig operator allowlist fixture");
    assert!(result.success(), "rig operator allowlist fixture failed");
}
