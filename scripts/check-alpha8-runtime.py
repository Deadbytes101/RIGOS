#!/usr/bin/env python3
import json
import os
import runpy
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render"
GATE = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-gate"
REMOTE_PROBE = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-remote-access-probe"


def write(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def verify_runtime_authority(root: Path) -> None:
    state = root / "state"
    runtime = root / "run"
    revision = state / "revisions/r1"
    (revision / "flight-sheets").mkdir(parents=True)
    (state / "current").symlink_to(Path("revisions/r1"))
    write(
        revision / "policy.json",
        {"schema": "rigos.policy/v1", "active_flight_sheet": "xmr"},
    )
    write(
        revision / "flight-sheets/xmr.json",
        {
            "schema": "rigos.flight-sheet/v1",
            "backend": "xmrig",
            "algorithm": "rx/0",
            "cpu": {
                "threads": 2,
                "huge_pages": True,
                "max_threads_hint": 100,
            },
        },
    )
    write(
        revision / "xmrig.json",
        {
            "cpu": {
                "enabled": True,
                "huge-pages": True,
                "max-threads-hint": 2,
            },
            "pools": [{"url": "pool.test:1", "algo": "rx/0"}],
            "http": {"enabled": False},
        },
    )
    environment = os.environ.copy()
    environment.update(
        {
            "RIGOS_STATE_PATH": str(state),
            "RIGOS_RUNTIME_PATH": str(runtime),
            "RIGOS_RENDER_SKIP_CHOWN": "1",
        }
    )
    subprocess.run(["python3", str(RENDERER)], env=environment, check=True)
    config = json.loads((runtime / "xmrig.json").read_text(encoding="utf-8"))
    assert config["cpu"]["max-threads-hint"] == 100
    assert config["cpu"]["rx"] == [-1, -1]
    status = json.loads(
        (runtime / "runtime-config-status.json").read_text(encoding="utf-8")
    )
    assert status["thread_mode"] == "exact"
    assert status["exact_threads"] == 2
    assert status["profile"] == "rx"
    allowed = subprocess.run(
        [
            "python3",
            str(GATE),
            "--state",
            str(state),
            "--runtime",
            str(runtime),
        ],
        check=False,
    )
    assert allowed.returncode == 0
    config["cpu"]["rx"] = [-1]
    write(runtime / "xmrig.json", config)
    denied = subprocess.run(
        [
            "python3",
            str(GATE),
            "--state",
            str(state),
            "--runtime",
            str(runtime),
        ],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    assert denied.returncode == 2


def verify_remote_truth(root: Path) -> None:
    runtime = root / "remote-run"
    runtime.mkdir()
    boot_id = root / "remote-boot-id"
    boot_id.write_text("boot-test\n", encoding="ascii")
    status_path = runtime / "recovery-access-status.json"
    write(
        status_path,
        {
            "schema": "rigos.recovery-access-status/v1",
            "boot_id": "boot-test",
            "state_outcome": "ready",
            "local_console_access": True,
        },
    )
    namespace = runpy.run_path(str(REMOTE_PROBE), run_name="rigos_remote_probe_test")
    globals_ = namespace["main"].__globals__
    globals_["RUNTIME"] = runtime
    globals_["STATUS"] = status_path
    globals_["BOOT_ID"] = boot_id
    globals_["unit_state"] = lambda _action, _unit: True
    globals_["has_listener"] = lambda path, _port: path.name == "tcp"
    assert namespace["main"]() == 0
    observed = json.loads(status_path.read_text(encoding="utf-8"))
    assert observed["mode"] == "operational"
    assert observed["remote_access"] == "active"
    assert observed["ssh_service_enabled"] is True
    assert observed["ssh_service_active"] is True
    assert observed["ssh_listener_ipv4"] is True
    assert observed["ssh_listener_ipv6"] is False
    globals_["has_listener"] = lambda _path, _port: False
    assert namespace["main"]() == 1
    degraded = json.loads(status_path.read_text(encoding="utf-8"))
    assert degraded["mode"] == "recovery"
    assert degraded["remote_access"] == "enabled_no_listener"


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="rigos-alpha8-") as temporary:
        root = Path(temporary)
        verify_runtime_authority(root)
        verify_remote_truth(root)
    print("RIGOS Alpha8 runtime and remote truth verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
