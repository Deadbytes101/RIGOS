#!/usr/bin/env python3
from __future__ import annotations

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, content: str, mode: int | None = None) -> None:
    target = ROOT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8", newline="\n")
    if mode is not None:
        target.chmod(mode)


def replace_once(path: str, old: str, new: str) -> None:
    content = read(path)
    count = content.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one patch anchor, found {count}")
    write(path, content.replace(old, new, 1))


def create(path: str, content: str, mode: int | None = None) -> None:
    target = ROOT / path
    if target.exists():
        raise SystemExit(f"refusing to overwrite existing file: {path}")
    write(path, content, mode)


# Exact RandomX thread intent. max-threads-hint remains a percentage hint.
replace_once(
    "crates/rigos-config/src/lib.rs",
    "pub fn build_runtime(\n",
    '''fn exact_cpu_profile_name(algorithm: &str) -> Option<&str> {
    if matches!(algorithm, "rx" | "rx/0") {
        Some("rx")
    } else if algorithm.starts_with("rx/") {
        Some(algorithm)
    } else {
        None
    }
}

fn exact_cpu_profile(count: u16) -> Value {
    Value::Array((0..count).map(|_| json!(-1)).collect())
}

pub fn build_runtime(
''',
)
replace_once(
    "crates/rigos-config/src/lib.rs",
    '''    match &sheet.cpu.threads {
        Threads::Auto(value) if value == "auto" => {}
        Threads::Count(value) if (1..=1024).contains(value) => {}
        _ => {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("cpu.threads"),
                "threads must be auto or 1 through 1024",
            ));
        }
    }
''',
    '''    match &sheet.cpu.threads {
        Threads::Auto(value) if value == "auto" => {}
        Threads::Count(value) if !(1..=1024).contains(value) => {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("cpu.threads"),
                "threads must be auto or 1 through 1024",
            ));
        }
        Threads::Count(_) if exact_cpu_profile_name(&sheet.algorithm).is_none() => {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("cpu.threads"),
                "explicit threads currently require a RandomX algorithm",
            ));
        }
        Threads::Count(_) => {}
        _ => {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("cpu.threads"),
                "threads must be auto or 1 through 1024",
            ));
        }
    }
''',
)
replace_once(
    "crates/rigos-config/src/lib.rs",
    '''    let mut cpu = Map::from_iter([
        ("enabled".into(), Value::Bool(true)),
        ("huge-pages".into(), Value::Bool(sheet.cpu.huge_pages)),
    ]);
    cpu.insert("max-threads-hint".into(), json!(sheet.cpu.max_threads_hint));
    if let Threads::Count(count) = sheet.cpu.threads {
        cpu.insert("max-threads-hint".into(), json!(count));
    }
    let xmrig = json!({"autosave":false,"background":false,"cpu":cpu,"pools":pools,"api":{"worker-id":worker},"http":{"enabled":false}});
''',
    '''    let mut cpu = Map::from_iter([
        ("enabled".into(), Value::Bool(true)),
        ("huge-pages".into(), Value::Bool(sheet.cpu.huge_pages)),
    ]);
    cpu.insert("max-threads-hint".into(), json!(sheet.cpu.max_threads_hint));
    if let Threads::Count(count) = &sheet.cpu.threads {
        let profile_name = exact_cpu_profile_name(&sheet.algorithm).ok_or_else(|| {
            error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                None,
                None,
                Some("cpu.threads"),
                "explicit threads do not have a safe XMRig profile mapping",
            )
        })?;
        cpu.insert(profile_name.into(), exact_cpu_profile(*count));
    }
    let xmrig = json!({"autosave":false,"background":false,"cpu":cpu,"pools":pools,"api":{"worker-id":worker},"http":{"enabled":false}});
''',
)
replace_once(
    "crates/rigos-config/src/lib.rs",
    '''    #[test]
    fn huge_pages_false_reaches_runtime_config() {
''',
    '''    #[test]
    fn explicit_randomx_threads_generate_exact_profile() {
        let proposal = Proposal {
            schema: "rigos.config-proposal/v1".into(),
            profile: parse_rig_profile(&profile("native", "FLIGHT_REF=xmr-ssl\\n")).unwrap(),
            flight_sheet: FlightSheet {
                schema: "rigos.flight-sheet/v1".into(),
                name: "xmr-ssl".into(),
                coin: "XMR".into(),
                backend: "xmrig".into(),
                algorithm: "rx/0".into(),
                pools: vec![Pool {
                    host: "pool.example".into(),
                    port: 443,
                    tls: true,
                    priority: 0,
                }],
                identity_ref: "main-xmr".into(),
                worker_template: "{node_name}".into(),
                cpu: CpuPolicy {
                    threads: Threads::Count(2),
                    huge_pages: true,
                    max_threads_hint: 100,
                },
            },
            provenance: None,
            source_sha256: "x".into(),
        };
        let identity = IdentityRecord {
            schema: "rigos.identity/v1".into(),
            alias: "main-xmr".into(),
            kind: "mining_identity".into(),
            value: "private-value".into(),
            created_locally: true,
        };
        let (_, xmrig) = build_runtime(&proposal, &identity).unwrap();
        assert_eq!(xmrig["cpu"]["max-threads-hint"], 100);
        assert_eq!(xmrig["cpu"]["rx"], json!([-1, -1]));

        let mut unsupported = proposal;
        unsupported.flight_sheet.algorithm = "cn/r".into();
        assert!(build_runtime(&unsupported, &identity).is_err());
    }

    #[test]
    fn huge_pages_false_reaches_runtime_config() {
''',
)

# Unprivileged inspection must use a redacted derived config, never the wallet-bearing runtime file.
replace_once(
    "crates/rigos-xmrig/src/lib.rs",
    '''        let config_path = cmdline
            .as_deref()
            .and_then(extract_config_path)
            .or_else(|| self.explicit_config.clone());
''',
    '''        let config_path = self.explicit_config.clone().or_else(|| {
            cmdline
                .as_deref()
                .and_then(extract_config_path)
        });
''',
)
replace_once(
    "crates/rigos-xmrig/src/lib.rs",
    '''        algorithm: raw.get("algo").and_then(Value::as_str).map(str::to_owned),
        huge_pages_requested: raw
            .get("randomx")
            .and_then(|v| v.get("huge-pages"))
            .and_then(Value::as_bool),
        thread_hint: raw.get("threads").and_then(Value::as_u64),
''',
    '''        algorithm: raw
            .get("algo")
            .and_then(Value::as_str)
            .or_else(|| {
                raw.get("pools")
                    .and_then(Value::as_array)
                    .and_then(|pools| pools.first())
                    .and_then(|pool| pool.get("algo"))
                    .and_then(Value::as_str)
            })
            .map(str::to_owned),
        huge_pages_requested: raw
            .get("randomx")
            .and_then(|v| v.get("huge-pages"))
            .and_then(Value::as_bool)
            .or_else(|| raw.pointer("/cpu/huge-pages").and_then(Value::as_bool)),
        thread_hint: raw
            .get("threads")
            .and_then(Value::as_u64)
            .or_else(|| {
                raw.pointer("/cpu/rx")
                    .and_then(Value::as_array)
                    .and_then(|threads| u64::try_from(threads.len()).ok())
            })
            .or_else(|| raw.pointer("/cpu/max-threads-hint").and_then(Value::as_u64)),
''',
)
replace_once(
    "crates/rigos-xmrig/src/lib.rs",
    '''    #[test]
    fn discovers_xmrig_from_synthetic_proc_without_mutation() {
''',
    '''    #[test]
    fn explicit_redacted_config_overrides_process_secret_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rigos-xmrig-explicit-{unique}"));
        let proc_root = root.join("proc");
        let pid_dir = proc_root.join("42");
        fs::create_dir_all(&pid_dir).unwrap();
        let secret = root.join("secret.json");
        let public = root.join("public.json");
        fs::write(&secret, r#"{"pools":[{"url":"secret-pool","user":"SENTINEL_SECRET"}]}"#).unwrap();
        fs::write(&public, r#"{"cpu":{"huge-pages":true,"rx":[-1,-1]},"pools":[{"url":"public-pool","algo":"rx/0"}],"http":{"enabled":false}}"#).unwrap();
        fs::write(pid_dir.join("comm"), "xmrig\\n").unwrap();
        fs::write(pid_dir.join("cmdline"), format!("xmrig\\0--config={}\\0", secret.display())).unwrap();
        fs::write(pid_dir.join("status"), "Name:\\txmrig\\nUid:\\t1000 1000 1000 1000\\n").unwrap();
        fs::write(pid_dir.join("cgroup"), "0::/system.slice/rigos-miner.service\\n").unwrap();
        fs::write(pid_dir.join("stat"), "42 (xmrig) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 100 0\\n").unwrap();
        fs::write(proc_root.join("uptime"), "100.0 0.0\\n").unwrap();
        let backend = XmrigBackend {
            explicit_executable: None,
            explicit_config: Some(public),
            probe_version: false,
        };
        let result = backend.discover(&MachineContext { proc_root, sys_root: root.join("sys") });
        let _ = fs::remove_dir_all(root);
        let snapshot = result.value.unwrap();
        assert_eq!(snapshot.config.pools, vec!["public-pool"]);
        assert_eq!(snapshot.config.algorithm.as_deref(), Some("rx/0"));
        assert_eq!(snapshot.config.huge_pages_requested, Some(true));
        assert_eq!(snapshot.config.thread_hint, Some(2));
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("SENTINEL_SECRET"));
    }

    #[test]
    fn discovers_xmrig_from_synthetic_proc_without_mutation() {
''',
)
replace_once(
    "crates/rigosd/src/lib.rs",
    '''        explicit_config: cli.xmrig_config,
''',
    '''        explicit_config: cli
            .xmrig_config
            .or_else(|| Some(PathBuf::from("/run/rigos/xmrig-public.json"))),
''',
)

# Appliance scripts and services.
create(
    "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-public-config",
    r'''#!/usr/bin/python3
import json
import os
import tempfile
from pathlib import Path

SOURCE = Path("/var/lib/rigos/current/xmrig.json")
TARGET = Path("/run/rigos/xmrig-public.json")
MAX_BYTES = 2 * 1024 * 1024


def fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def main() -> int:
    raw = SOURCE.read_bytes()
    if len(raw) > MAX_BYTES:
        raise RuntimeError("XMRig configuration exceeds the public-view size limit")
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise RuntimeError("XMRig configuration is not an object")
    for pool in value.get("pools", []):
        if isinstance(pool, dict):
            pool.pop("user", None)
            pool.pop("pass", None)
    http = value.get("http")
    if isinstance(http, dict):
        http.pop("access-token", None)
    value["rigos-public-view"] = {
        "schema": "rigos.xmrig-public-config/v1",
        "source": "active_revision",
        "identity_redacted": True,
    }
    TARGET.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=TARGET.parent, prefix=".xmrig-public-", delete=False
    ) as stream:
        json.dump(value, stream, sort_keys=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
        temporary = Path(stream.name)
    temporary.chmod(0o644)
    os.replace(temporary, TARGET)
    fsync_directory(TARGET.parent)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
''',
    0o755,
)
create(
    "build/usb/includes.chroot/usr/lib/rigos/rigos-ssh-identity",
    r'''#!/usr/bin/python3
import hashlib
import json
import os
import stat
import subprocess
import tempfile
from pathlib import Path

RUNTIME = Path("/run/rigos")
STATE = Path("/var/lib/rigos")
SSH_DIRECTORY = Path("/etc/ssh")
BOOT_ID = Path("/proc/sys/kernel/random/boot_id")
STORE = STATE / "machine" / "ssh-host-keys"
STATUS = RUNTIME / "ssh-identity-status.json"
KEY_TYPES = ("rsa", "ecdsa", "ed25519")
MAX_KEY_BYTES = 1024 * 1024


def key_paths(root: Path):
    result = []
    for key_type in KEY_TYPES:
        private = root / f"ssh_host_{key_type}_key"
        result.append((private, 0o600))
        result.append((Path(f"{private}.pub"), 0o644))
    return result


def fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def ensure_directory(path: Path, mode: int) -> None:
    path.mkdir(mode=mode, parents=True, exist_ok=True)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode):
        raise RuntimeError(f"unsafe directory: {path}")
    if info.st_uid != os.geteuid():
        raise RuntimeError(f"directory owner mismatch: {path}")
    path.chmod(mode)


def valid_key(path: Path, mode: int) -> bool:
    try:
        info = path.lstat()
    except FileNotFoundError:
        return False
    return (
        stat.S_ISREG(info.st_mode)
        and not stat.S_ISLNK(info.st_mode)
        and info.st_uid == os.geteuid()
        and stat.S_IMODE(info.st_mode) == mode
        and 0 < info.st_size <= MAX_KEY_BYTES
    )


def key_set_state(root: Path) -> str:
    try:
        info = root.lstat()
    except FileNotFoundError:
        return "absent"
    if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        return "invalid"
    expected = {path.name for path, _mode in key_paths(root)}
    observed = {entry.name for entry in root.iterdir()}
    if not observed:
        return "absent"
    if observed != expected:
        return "invalid"
    return "complete" if all(valid_key(path, mode) for path, mode in key_paths(root)) else "invalid"


def atomic_copy(source: Path, destination: Path, mode: int) -> None:
    data = source.read_bytes()
    if not data or len(data) > MAX_KEY_BYTES:
        raise RuntimeError(f"invalid SSH host key size: {source}")
    descriptor, name = tempfile.mkstemp(prefix=f".{destination.name}-", dir=destination.parent)
    temporary = Path(name)
    try:
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "wb", closefd=True) as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, destination)
        destination.chmod(mode)
        fsync_directory(destination.parent)
    finally:
        temporary.unlink(missing_ok=True)


def live_keys_ready() -> bool:
    return all(valid_key(path, mode) for path, mode in key_paths(SSH_DIRECTORY))


def remove_live_keys() -> None:
    for path, _mode in key_paths(SSH_DIRECTORY):
        path.unlink(missing_ok=True)


def generate_live_keys() -> None:
    SSH_DIRECTORY.mkdir(mode=0o755, parents=True, exist_ok=True)
    remove_live_keys()
    result = subprocess.run(["/usr/bin/ssh-keygen", "-A"], check=False)
    if result.returncode != 0 or not live_keys_ready():
        raise RuntimeError("SSH host key generation failed")


def persist_live_keys() -> None:
    ensure_directory(STORE, 0o700)
    for source, mode in key_paths(SSH_DIRECTORY):
        if not valid_key(source, mode):
            raise RuntimeError(f"live SSH host key is invalid: {source}")
        atomic_copy(source, STORE / source.name, mode)
    if key_set_state(STORE) != "complete":
        raise RuntimeError("persistent SSH host key store is incomplete")


def restore_live_keys() -> None:
    if key_set_state(STORE) != "complete":
        raise RuntimeError("persistent SSH host key store is unavailable")
    SSH_DIRECTORY.mkdir(mode=0o755, parents=True, exist_ok=True)
    remove_live_keys()
    for source, mode in key_paths(STORE):
        atomic_copy(source, SSH_DIRECTORY / source.name, mode)
    if not live_keys_ready():
        raise RuntimeError("restored SSH host keys failed validation")


def write_status(value: dict) -> None:
    RUNTIME.mkdir(mode=0o755, parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=RUNTIME, prefix=".ssh-identity-", delete=False) as stream:
        json.dump(value, stream, sort_keys=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
        temporary = Path(stream.name)
    temporary.chmod(0o644)
    os.replace(temporary, STATUS)
    fsync_directory(RUNTIME)


def main() -> int:
    boot_id = BOOT_ID.read_text(encoding="ascii").strip()
    try:
        state_status = json.loads((RUNTIME / "state-status.json").read_text(encoding="utf-8"))
        if state_status.get("schema") != "rigos.state-status/v1" or state_status.get("boot_id") != boot_id or state_status.get("outcome") != "ready":
            raise RuntimeError("persistent state is not ready for this boot")
        persistent_state = key_set_state(STORE)
        if persistent_state == "complete":
            restore_live_keys()
            action = "restored"
        elif persistent_state == "absent":
            generate_live_keys()
            persist_live_keys()
            action = "created"
        else:
            raise RuntimeError("persistent SSH host key store is incomplete or unsafe")
        fingerprints = {
            key_type: hashlib.sha256((SSH_DIRECTORY / f"ssh_host_{key_type}_key.pub").read_bytes()).hexdigest()
            for key_type in KEY_TYPES
        }
        write_status({"schema":"rigos.ssh-identity-status/v1","boot_id":boot_id,"outcome":"ready","action":action,"persisted":True,"public_key_sha256":fingerprints,"reason":None})
        return 0
    except (OSError, ValueError, json.JSONDecodeError, RuntimeError) as error:
        write_status({"schema":"rigos.ssh-identity-status/v1","boot_id":boot_id,"outcome":"error","action":None,"persisted":False,"public_key_sha256":{},"reason":str(error)})
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
''',
    0o755,
)
create(
    "build/usb/includes.chroot/usr/lib/rigos/rigos-remote-access-probe",
    r'''#!/usr/bin/python3
import json
import os
import subprocess
import tempfile
from pathlib import Path

RUNTIME = Path("/run/rigos")
BOOT_ID = Path("/proc/sys/kernel/random/boot_id")
STATUS = RUNTIME / "recovery-access-status.json"
IDENTITY_STATUS = RUNTIME / "ssh-identity-status.json"
SSH_PORT = 22


def unit_state(action: str, name: str) -> bool:
    return subprocess.run(["/usr/bin/systemctl", action, "--quiet", name], check=False).returncode == 0


def tcp_listener(path: Path, port: int) -> bool:
    try:
        lines = path.read_text(encoding="ascii").splitlines()[1:]
    except OSError:
        return False
    for line in lines:
        fields = line.split()
        if len(fields) < 4 or fields[3] != "0A":
            continue
        try:
            observed_port = int(fields[1].rsplit(":", 1)[1], 16)
        except (IndexError, ValueError):
            continue
        if observed_port == port:
            return True
    return False


def fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def main() -> int:
    boot_id = BOOT_ID.read_text(encoding="ascii").strip()
    status = json.loads(STATUS.read_text(encoding="utf-8"))
    identity = json.loads(IDENTITY_STATUS.read_text(encoding="utf-8"))
    if status.get("boot_id") != boot_id or identity.get("boot_id") != boot_id:
        raise RuntimeError("remote access evidence belongs to another boot")
    enabled = unit_state("is-enabled", "ssh.service")
    active = unit_state("is-active", "ssh.service")
    listener_ipv4 = tcp_listener(Path("/proc/net/tcp"), SSH_PORT)
    listener_ipv6 = tcp_listener(Path("/proc/net/tcp6"), SSH_PORT)
    listening = listener_ipv4 or listener_ipv6
    if enabled and active and listening:
        remote_access = "active"
    elif enabled and not listening:
        remote_access = "enabled_no_listener"
    elif listening and not active:
        remote_access = "listener_without_service"
    else:
        remote_access = "inactive"
    operational = status.get("state_outcome") == "ready" and status.get("local_console_access") is True and identity.get("outcome") == "ready" and identity.get("persisted") is True and remote_access == "active"
    status.update({"mode":"operational" if operational else "recovery","remote_access":remote_access,"remote_protocol":"ssh","remote_port":SSH_PORT,"ssh_service_enabled":enabled,"ssh_service_active":active,"ssh_listener_ipv4":listener_ipv4,"ssh_listener_ipv6":listener_ipv6,"ssh_host_key_action":identity.get("action"),"ssh_host_keys_persisted":identity.get("persisted") is True})
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=RUNTIME, prefix=".recovery-access-", delete=False) as stream:
        json.dump(status, stream, sort_keys=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
        temporary = Path(stream.name)
    temporary.chmod(0o644)
    os.replace(temporary, STATUS)
    fsync_directory(RUNTIME)
    return 0 if operational else 1


if __name__ == "__main__":
    raise SystemExit(main())
''',
    0o755,
)

create("build/usb/includes.chroot/etc/systemd/system/rigos-ssh-identity.service", '''[Unit]
Description=Establish persistent RIGOS SSH machine identity
After=rigos-state-ready.service
Requires=rigos-state-ready.service
Before=ssh.service

[Service]
Type=oneshot
ExecStart=/usr/lib/rigos/rigos-ssh-identity
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
''')
create("build/usb/includes.chroot/etc/systemd/system/rigos-remote-access-status.service", '''[Unit]
Description=Observe RIGOS remote access truth
After=network-online.target ssh.service rigos-ssh-identity.service
Wants=network-online.target
Requires=ssh.service rigos-ssh-identity.service

[Service]
Type=oneshot
ExecStart=/usr/lib/rigos/rigos-remote-access-probe
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
''')
create("build/usb/includes.chroot/etc/systemd/system/rigos-miner-public-status.service", '''[Unit]
Description=Publish redacted RIGOS miner configuration truth
After=rigos-miner.service
Requires=rigos-state-ready.service
ConditionPathExists=/var/lib/rigos/xmrig.json

[Service]
Type=oneshot
ExecStart=/usr/lib/rigos/rigos-miner-public-config
''')
create("build/usb/includes.chroot/etc/systemd/system/ssh.service.d/rigos.conf", '''[Unit]
Requires=rigos-ssh-identity.service
After=rigos-ssh-identity.service
''')
create("build/usb/includes.chroot/etc/ssh/sshd_config.d/rigos.conf", '''PasswordAuthentication yes
KbdInteractiveAuthentication no
PermitRootLogin no
AllowUsers rigosadmin
X11Forwarding no
AllowAgentForwarding no
AllowTcpForwarding no
PermitTunnel no
GatewayPorts no
''')

replace_once(
    "build/usb/hooks/010-rigos.chroot",
    "install -d -m 0755 /usr/lib/rigos\n",
    '''install -d -m 0755 /usr/lib/rigos /usr/local/bin
ln -sfn /usr/lib/rigos/rigosd /usr/local/bin/rigosd
ln -sfn /usr/lib/rigos/rigosctl /usr/local/bin/rigosctl
rm -f /etc/ssh/ssh_host_rsa_key /etc/ssh/ssh_host_rsa_key.pub
rm -f /etc/ssh/ssh_host_ecdsa_key /etc/ssh/ssh_host_ecdsa_key.pub
rm -f /etc/ssh/ssh_host_ed25519_key /etc/ssh/ssh_host_ed25519_key.pub
''',
)
replace_once(
    "build/usb/hooks/010-rigos.chroot",
    "/usr/lib/rigos/rigos-identity-seed /usr/lib/rigos/xmrig\n",
    "/usr/lib/rigos/rigos-identity-seed /usr/lib/rigos/rigos-ssh-identity /usr/lib/rigos/rigos-remote-access-probe /usr/lib/rigos/rigos-miner-public-config /usr/lib/rigos/xmrig\n",
)
replace_once(
    "build/usb/hooks/010-rigos.chroot",
    "systemctl enable NetworkManager.service rigos-state.service rigos-recovery-access.service rigos-state-ready.service rigos-profile-apply.service rigos-hugepages.service rigos-firstboot.service rigos-miner.service tmp.mount\n",
    "systemctl enable NetworkManager.service ssh.service rigos-state.service rigos-recovery-access.service rigos-state-ready.service rigos-profile-apply.service rigos-ssh-identity.service rigos-remote-access-status.service rigos-hugepages.service rigos-firstboot.service rigos-miner.service tmp.mount\nsystemctl disable ssh.socket 2>/dev/null || true\n",
)
replace_once(
    "build/usb/includes.chroot/etc/systemd/system/rigos-miner.service",
    "Wants=network-online.target\n",
    "Wants=network-online.target rigos-miner-public-status.service\n",
)

# Ordering and verification gates.
replace_once(
    "scripts/verify-systemd-ordering.py",
    '''        "rigos-state-ready.service", "rigos-profile-apply.service",
        "rigos-firstboot.service", "rigos-hugepages.service", "rigos-miner.service",
''',
    '''        "rigos-state-ready.service", "rigos-profile-apply.service",
        "rigos-firstboot.service", "rigos-ssh-identity.service",
        "rigos-remote-access-status.service", "rigos-miner-public-status.service",
        "rigos-hugepages.service", "rigos-miner.service",
''',
)
replace_once(
    "scripts/verify-systemd-ordering.py",
    '    ready = units["rigos-state-ready.service"]\n',
    '''    ssh_identity = units["rigos-ssh-identity.service"]
    includes(ssh_identity.words("Unit", "After"), {"rigos-state-ready.service"}, "SSH identity must follow state readiness")
    includes(ssh_identity.words("Unit", "Requires"), {"rigos-state-ready.service"}, "SSH identity must require state readiness")
    includes(ssh_identity.words("Unit", "Before"), {"ssh.service"}, "SSH identity must precede sshd")

    remote_access = units["rigos-remote-access-status.service"]
    includes(remote_access.words("Unit", "After"), {"ssh.service", "rigos-ssh-identity.service"}, "remote access truth ordering is incomplete")
    includes(remote_access.words("Unit", "Requires"), {"ssh.service", "rigos-ssh-identity.service"}, "remote access truth dependencies are incomplete")

    public_status = units["rigos-miner-public-status.service"]
    includes(public_status.words("Unit", "After"), {"rigos-miner.service"}, "public miner status must follow miner")
    includes(public_status.words("Unit", "Requires"), {"rigos-state-ready.service"}, "public miner status must require ready state")

    ready = units["rigos-state-ready.service"]
''',
)
replace_once(
    "scripts/verify.sh",
    '''  build/usb/includes.chroot/usr/lib/rigos/rigos-miner-gate \\
  scripts/verify-systemd-ordering.py
''',
    '''  build/usb/includes.chroot/usr/lib/rigos/rigos-miner-gate \\
  build/usb/includes.chroot/usr/lib/rigos/rigos-ssh-identity \\
  build/usb/includes.chroot/usr/lib/rigos/rigos-remote-access-probe \\
  build/usb/includes.chroot/usr/lib/rigos/rigos-miner-public-config \\
  scripts/verify-systemd-ordering.py
''',
)
replace_once(
    "scripts/verify.sh",
    "grep -Fq 'ExecCondition=/usr/lib/rigos/rigos-miner-gate' build/usb/includes.chroot/etc/systemd/system/rigos-miner.service\n",
    '''grep -Fq 'ExecCondition=/usr/lib/rigos/rigos-miner-gate' build/usb/includes.chroot/etc/systemd/system/rigos-miner.service
grep -Fq 'rigos-miner-public-status.service' build/usb/includes.chroot/etc/systemd/system/rigos-miner.service
grep -Fq 'Before=ssh.service' build/usb/includes.chroot/etc/systemd/system/rigos-ssh-identity.service
grep -Fq 'Requires=rigos-ssh-identity.service' build/usb/includes.chroot/etc/systemd/system/ssh.service.d/rigos.conf
grep -Fq 'After=network-online.target ssh.service rigos-ssh-identity.service' build/usb/includes.chroot/etc/systemd/system/rigos-remote-access-status.service
grep -Fq 'ln -sfn /usr/lib/rigos/rigosd /usr/local/bin/rigosd' build/usb/hooks/010-rigos.chroot
grep -Fq 'ln -sfn /usr/lib/rigos/rigosctl /usr/local/bin/rigosctl' build/usb/hooks/010-rigos.chroot
grep -Fq 'systemctl disable ssh.socket' build/usb/hooks/010-rigos.chroot
grep -Fqx 'AllowUsers rigosadmin' build/usb/includes.chroot/etc/ssh/sshd_config.d/rigos.conf
grep -Fqx 'PermitRootLogin no' build/usb/includes.chroot/etc/ssh/sshd_config.d/rigos.conf
''',
)

# Keep the image verifier authoritative without requiring a physical boot.
replace_once(
    "scripts/verify-usb-appliance.sh",
    "  etc/systemd/system/rigos-recovery-access.service \\\n",
    "  etc/systemd/system/rigos-recovery-access.service \\\n  etc/systemd/system/rigos-ssh-identity.service \\\n  etc/systemd/system/rigos-remote-access-status.service \\\n  etc/systemd/system/rigos-miner-public-status.service \\\n  etc/systemd/system/ssh.service.d/rigos.conf \\\n  etc/ssh/sshd_config.d/rigos.conf \\\n",
)
replace_once(
    "scripts/verify-usb-appliance.sh",
    "  usr/lib/rigos/rigosd usr/lib/rigos/rigosctl \\\n",
    "  usr/lib/rigos/rigosd usr/lib/rigos/rigosctl usr/local/bin/rigosd usr/local/bin/rigosctl \\\n",
)
replace_once(
    "scripts/verify-usb-appliance.sh",
    "usr/lib/rigos/rigos-miner-gate usr/lib/rigos/xmrig",
    "usr/lib/rigos/rigos-miner-gate usr/lib/rigos/rigos-ssh-identity usr/lib/rigos/rigos-remote-access-probe usr/lib/rigos/rigos-miner-public-config usr/lib/rigos/xmrig",
)
replace_once(
    "scripts/verify-usb-appliance.sh",
    'python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-miner-gate"\n',
    '''python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-miner-gate"
python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-ssh-identity"
python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-remote-access-probe"
python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-miner-public-config"
''',
)
replace_once(
    "scripts/verify-usb-appliance.sh",
    '''rigosctl_path="$(PATH="$temporary/root/usr/local/sbin:$temporary/root/usr/bin" command -v rigosctl)"
[[ "$rigosctl_path" == "$temporary/root/usr/local/sbin/rigosctl" && -x "$rigosctl_path" ]] || die 'rigosctl is not executable in the appliance PATH'
''',
    '''user_path="$temporary/root/usr/local/bin:$temporary/root/usr/bin:$temporary/root/bin"
rigosd_path="$(PATH="$user_path" command -v rigosd)"
rigosctl_path="$(PATH="$user_path" command -v rigosctl)"
[[ "$rigosd_path" == "$temporary/root/usr/local/bin/rigosd" && -x "$rigosd_path" ]] || die 'rigosd is not executable in the user appliance PATH'
[[ "$rigosctl_path" == "$temporary/root/usr/local/bin/rigosctl" && -x "$rigosctl_path" ]] || die 'rigosctl is not executable in the user appliance PATH'
''',
)
replace_once(
    "scripts/verify-usb-appliance.sh",
    'unsquashfs -no-progress -d "$temporary/root" "$temporary/a/live/filesystem.squashfs" \\\n',
    '''if unsquashfs -ll "$temporary/a/live/filesystem.squashfs" | grep -Eq '/etc/ssh/ssh_host_(rsa|ecdsa|ed25519)_key$'; then
  die 'image contains a baked SSH private host key'
fi
unsquashfs -no-progress -d "$temporary/root" "$temporary/a/live/filesystem.squashfs" \\
''',
)

# Remove the one-shot patch mechanism from the resulting source commit.
(ROOT / "scripts/apply-alpha8-physical-fixes.py").unlink()
workflow = ROOT / ".github/workflows/alpha8-apply.yml"
workflow.unlink()
print("Alpha8 physical findings applied")
