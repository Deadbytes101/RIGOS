#!/usr/bin/env python3
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "build/usb/includes.chroot/etc/ssh/sshd_config.d/00-rigos.conf"
PACKAGES = ROOT / "build/usb/package-lists/rigos.list.chroot"
HOOK = ROOT / "build/usb/hooks/010-rigos.chroot"
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
    print("RIGOS Alpha8 SSH hotfix verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
