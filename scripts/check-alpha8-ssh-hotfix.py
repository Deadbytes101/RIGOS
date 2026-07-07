#!/usr/bin/env python3
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "build/usb/includes.chroot/etc/ssh/sshd_config.d/00-rigos.conf"
PACKAGES = ROOT / "build/usb/package-lists/rigos.list.chroot"
HOOK = ROOT / "build/usb/hooks/010-rigos.chroot"
DOCKERFILE = ROOT / "build/usb/Dockerfile"
RECOVERY_UNIT = ROOT / "build/usb/includes.chroot/etc/systemd/system/rigos-recovery-access.service"
RECOVERY_GATE = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-recovery-access-verify"
EXPECTED_POLICY_SHA256 = "d59b6bcc078a047d1f1cc90ef6ed9205476d91f874be809009bdd442ef66b8c3"


def normalized_lf_bytes(path: Path) -> bytes:
    raw = path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf"):
        raise RuntimeError("Alpha8 SSH policy must be UTF-8 without BOM")
    return raw.replace(b"\r\n", b"\n").replace(b"\r", b"\n")


def main() -> int:
    policy = normalized_lf_bytes(POLICY)
    observed = hashlib.sha256(policy).hexdigest()
    if observed != EXPECTED_POLICY_SHA256:
        raise RuntimeError(
            f"Alpha8 SSH policy hash mismatch: expected={EXPECTED_POLICY_SHA256} observed={observed}"
        )
    packages = PACKAGES.read_text(encoding="utf-8").splitlines()
    if "openssh-server" not in packages:
        raise RuntimeError("OpenSSH server package is missing")
    hook = HOOK.read_text(encoding="utf-8")
    if "ssh.service" not in hook or "systemctl disable ssh.socket" not in hook:
        raise RuntimeError("deterministic SSH service wiring is missing")
    dockerfile = DOCKERFILE.read_text(encoding="utf-8")
    if 'ENV PATH="/usr/local/cargo/bin:/usr/local/rustup/bin:${PATH}"' not in dockerfile:
        raise RuntimeError("builder Cargo PATH is not explicit")
    if 'ENTRYPOINT ["/bin/bash", "-c",' not in dockerfile:
        raise RuntimeError("builder entrypoint must use a non-login shell")
    if '"bash", "-lc"' in dockerfile or '"/bin/bash", "-lc"' in dockerfile:
        raise RuntimeError("builder entrypoint must not use a login shell")
    if "cargo --version" not in dockerfile or "rustc --version" not in dockerfile:
        raise RuntimeError("builder toolchain verification is missing")

    recovery_unit = RECOVERY_UNIT.read_text(encoding="utf-8")
    required_unit_lines = (
        "Before=rigos-state-ready.service rigos-firstboot.service getty@tty1.service ssh.service",
        "SuccessExitStatus=1",
        "ExecStartPost=/usr/bin/python3 /usr/lib/rigos/rigos-recovery-access-verify",
    )
    for required in required_unit_lines:
        if required not in recovery_unit:
            raise RuntimeError(f"recovery access hotfix wiring is missing: {required}")

    recovery_gate = RECOVERY_GATE.read_text(encoding="utf-8")
    compile(recovery_gate, str(RECOVERY_GATE), "exec")
    for required in (
        'status.get("boot_id") != boot_id',
        'status.get("local_console_access") is not True',
        'status.get("credential_persisted") is not True',
    ):
        if required not in recovery_gate:
            raise RuntimeError(f"recovery access validator contract is missing: {required}")

    print("RIGOS Alpha8 SSH and recovery hotfix verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
