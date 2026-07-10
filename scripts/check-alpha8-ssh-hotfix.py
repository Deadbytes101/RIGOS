#!/usr/bin/env python3
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "build/usb/includes.chroot/etc/ssh/sshd_config.d/00-rigos.conf"
HOSTKEY_POLICY = ROOT / "build/usb/includes.chroot/etc/ssh/sshd_config.d/01-rigos-hostkeys.conf"
PACKAGES = ROOT / "build/usb/package-lists/rigos.list.chroot"
HOOK = ROOT / "build/usb/hooks/010-rigos.chroot"
DOCKERFILE = ROOT / "build/usb/Dockerfile"
RECOVERY_UNIT = ROOT / "build/usb/includes.chroot/etc/systemd/system/rigos-recovery-access.service"
RECOVERY_AUTHORITY = ROOT / "build/usb/includes.chroot/usr/local/sbin/rigos-recovery-access"
RECOVERY_GATE = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-recovery-access-verify"
STATE_READY_UNIT = ROOT / "build/usb/includes.chroot/etc/systemd/system/rigos-state-ready.service"
HOSTKEY_UNIT = ROOT / "build/usb/includes.chroot/etc/systemd/system/rigos-ssh-hostkeys.service"
SSH_DROPIN = ROOT / "build/usb/includes.chroot/etc/systemd/system/ssh.service.d/rigos-observe.conf"
HOSTKEY_AUTHORITY = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-ssh-hostkeys"
SSH_DIRECTORY = ROOT / "build/usb/includes.chroot/etc/ssh"
EXPECTED_POLICY_SHA256 = "d59b6bcc078a047d1f1cc90ef6ed9205476d91f874be809009bdd442ef66b8c3"


def normalized_lf_bytes(path: Path) -> bytes:
    raw = path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf"):
        raise RuntimeError(f"{path.name} must be UTF-8 without BOM")
    return raw.replace(b"\r\n", b"\n").replace(b"\r", b"\n")


def require_lines(path: Path, required_lines: tuple[str, ...]) -> None:
    lines = set(path.read_text(encoding="utf-8").splitlines())
    for required in required_lines:
        if required not in lines:
            raise RuntimeError(f"{path.name} contract is missing: {required}")


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

    hostkey_policy = normalized_lf_bytes(HOSTKEY_POLICY)
    expected_hostkey_policy = (
        b"HostKey /run/rigos/ssh-hostkeys/ssh_host_ed25519_key\n"
        b"PasswordAuthentication yes\n"
        b"PermitRootLogin no\n"
        b"AllowUsers rigosadmin\n"
    )
    if hostkey_policy != expected_hostkey_policy:
        raise RuntimeError("diagnostic SSH HostKey and authentication policy is not exact")
    baked_keys = sorted(path for path in SSH_DIRECTORY.glob("ssh_host_*_key*") if path.is_file())
    if baked_keys:
        raise RuntimeError("appliance source contains a baked SSH host private or public key")

    hook = HOOK.read_text(encoding="utf-8")
    for required in (
        "ssh.service",
        "systemctl disable ssh.socket",
        "rigos-ssh-hostkeys.service",
        "/usr/lib/rigos/rigos-ssh-hostkeys",
        "rm -f /etc/ssh/ssh_host_*_key /etc/ssh/ssh_host_*_key.pub",
        "install -d -o root -g rigos -m 0750 /var/lib/rigos",
    ):
        if required not in hook:
            raise RuntimeError(f"deterministic SSH service wiring is missing: {required}")

    authority = HOSTKEY_AUTHORITY.read_text(encoding="utf-8")
    compile(authority, str(HOSTKEY_AUTHORITY), "exec")
    for required in (
        'STATE = Path("/var/lib/rigos")',
        'KEYS = SYSTEM / "ssh-hostkeys"',
        'ACTIVE_KEYS = RUNTIME / "ssh-hostkeys"',
        '"schema": "rigos.ssh-hostkeys/v1"',
        '"schema": "rigos.ssh-active-hostkeys/v1"',
        'mode = "persistent"',
        'mode = "ephemeral"',
        'or status.get("outcome") != "ready"',
        '"SSH public and private keys do not match"',
        '"persistent SSH host identity exists without a valid manifest"',
        '"ephemeral SSH host identity generation failed"',
    ):
        if required not in authority:
            raise RuntimeError(f"SSH host-key authority contract is missing: {required}")

    require_lines(
        HOSTKEY_UNIT,
        (
            "After=rigos-state-ready.service",
            "Wants=rigos-state-ready.service",
            "Before=ssh.service",
            "ExecStart=/usr/lib/rigos/rigos-ssh-hostkeys",
            "ReadWritePaths=/var/lib/rigos /run/rigos",
            "WantedBy=multi-user.target",
        ),
    )
    if "Requires=rigos-state-ready.service" in HOSTKEY_UNIT.read_text(encoding="utf-8"):
        raise RuntimeError("diagnostic SSH is still hard-blocked by state readiness")
    require_lines(
        SSH_DROPIN,
        (
            "After=rigos-recovery-access.service rigos-ssh-hostkeys.service",
            "Requires=rigos-ssh-hostkeys.service",
            "Wants=rigos-remote-access-observe.service",
        ),
    )
    if "Before=rigos-ssh-hostkeys.service" not in STATE_READY_UNIT.read_text(encoding="utf-8"):
        raise RuntimeError("state readiness attempt is not ordered before SSH identity selection")

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

    recovery_authority = RECOVERY_AUTHORITY.read_text(encoding="utf-8")
    compile(recovery_authority, str(RECOVERY_AUTHORITY), "exec")
    for required in (
        "def persistent_store_ready(status: dict) -> bool:",
        '"credential_scope": "persistent" if persistent else "boot"',
        "credential_persisted = persistent and CREDENTIAL_FILE.is_file()",
        "if persistent:",
        "This password is not persistent",
    ):
        if required not in recovery_authority:
            raise RuntimeError(f"recovery credential authority contract is missing: {required}")

    recovery_gate = RECOVERY_GATE.read_text(encoding="utf-8")
    compile(recovery_gate, str(RECOVERY_GATE), "exec")
    for required in (
        'status.get("boot_id") != boot_id',
        'status.get("local_console_access") is not True',
        'scope = status.get("credential_scope")',
        'if scope == "persistent":',
        'elif scope == "boot":',
        'return deny("boot_credential_claims_persistence")',
    ):
        if required not in recovery_gate:
            raise RuntimeError(f"recovery access validator contract is missing: {required}")

    print("RIGOS SSH, recovery, and diagnostic host-key verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
