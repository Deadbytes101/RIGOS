#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected exactly one match in {path}, found {count}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_xmrig_backend() -> None:
    path = ROOT / "crates/rigos-xmrig/src/lib.rs"
    replace_once(
        path,
        """        let config_path = cmdline
            .as_deref()
            .and_then(extract_config_path)
            .or_else(|| self.explicit_config.clone());""",
        """        let config_path = self
            .explicit_config
            .clone()
            .or_else(|| cmdline.as_deref().and_then(extract_config_path));""",
    )

    marker = """    #[test]
    fn discovers_xmrig_from_synthetic_proc_without_mutation() {"""
    test = """    #[test]
    fn explicit_config_overrides_process_cmdline_config() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rigos-proc-explicit-{unique}"));
        let proc_root = root.join("proc");
        let pid_dir = proc_root.join("42");
        fs::create_dir_all(&pid_dir).unwrap();
        let private = root.join("private.json");
        let public = root.join("public.json");
        fs::write(
            &private,
            r#"{"algo":"rx/private","http":{"enabled":false}}"#,
        )
        .unwrap();
        fs::write(
            &public,
            r#"{"algo":"rx/0","threads":2,"randomx":{"huge-pages":true},"http":{"enabled":false}}"#,
        )
        .unwrap();
        fs::write(pid_dir.join("comm"), "xmrig\n").unwrap();
        fs::write(
            pid_dir.join("cmdline"),
            format!("xmrig\0--config={}\0", private.display()),
        )
        .unwrap();
        fs::write(
            pid_dir.join("status"),
            "Name:\txmrig\nUid:\t1000 1000 1000 1000\n",
        )
        .unwrap();
        fs::write(pid_dir.join("cgroup"), "0::/system.slice/xmrig.service\n").unwrap();
        fs::write(
            pid_dir.join("stat"),
            "42 (xmrig) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 100 0\n",
        )
        .unwrap();
        fs::write(proc_root.join("uptime"), "100.0 0.0\n").unwrap();

        let expected_path = public.to_string_lossy().into_owned();
        let backend = XmrigBackend {
            explicit_executable: None,
            explicit_config: Some(public),
            probe_version: false,
        };
        let result = backend.discover(&MachineContext {
            proc_root,
            sys_root: root.join("sys"),
        });
        let _ = fs::remove_dir_all(root);
        let snapshot = result.value.unwrap();
        assert_eq!(snapshot.config.path, Some(expected_path));
        assert_eq!(snapshot.config.algorithm.as_deref(), Some("rx/0"));
        assert_eq!(snapshot.config.thread_hint, Some(2));
        assert_eq!(snapshot.config.huge_pages_requested, Some(true));
    }

"""
    replace_once(path, marker, test + marker)


def patch_runtime_renderer() -> None:
    path = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render"
    helper_marker = """def render() -> tuple[dict, dict, dict]:"""
    helpers = """def public_pool(pool: dict) -> dict:
    safe = {}
    url = pool.get("url")
    if isinstance(url, str) and url:
        safe["url"] = url.rsplit("@", 1)[-1]
    algo = pool.get("algo")
    if isinstance(algo, str) and algo:
        safe["algo"] = algo
    for key in ("tls", "nicehash", "keepalive"):
        value = pool.get(key)
        if isinstance(value, bool):
            safe[key] = value
    priority = pool.get("priority")
    if isinstance(priority, int) and not isinstance(priority, bool):
        safe["priority"] = priority
    return safe


def public_http(http: object) -> dict:
    if not isinstance(http, dict):
        return {"enabled": False}
    safe = {}
    enabled = http.get("enabled")
    safe["enabled"] = enabled if isinstance(enabled, bool) else False
    host = http.get("host")
    if isinstance(host, str) and host:
        safe["host"] = host
    port = http.get("port")
    if isinstance(port, int) and not isinstance(port, bool) and 1 <= port <= 65535:
        safe["port"] = port
    restricted = http.get("restricted")
    if isinstance(restricted, bool):
        safe["restricted"] = restricted
    return safe


"""
    replace_once(path, helper_marker, helpers + helper_marker)

    replace_once(
        path,
        """    public = copy.deepcopy(runtime)
    for pool in public.get("pools", []):
        if isinstance(pool, dict):
            pool.pop("user", None)
            pool.pop("pass", None)
    http = public.get("http")
    if isinstance(http, dict):
        http.pop("access-token", None)

    public["algo"] = algorithm
    if exact_threads is not None:
        public["threads"] = exact_threads
    else:
        public.pop("threads", None)
    public_randomx = public.get("randomx")
    if not isinstance(public_randomx, dict):
        public_randomx = {}
    huge_pages = cpu.get("huge-pages")
    if isinstance(huge_pages, bool):
        public_randomx["huge-pages"] = huge_pages
    public["randomx"] = public_randomx
    public["rigos-public-view"] = {
        "schema": "rigos.xmrig-public-config/v1",
        "identity_redacted": True,
        "source_revision": current.name,
    }
""",
        """    huge_pages = cpu.get("huge-pages")
    public_cpu = {}
    enabled = cpu.get("enabled")
    if isinstance(enabled, bool):
        public_cpu["enabled"] = enabled
    if isinstance(huge_pages, bool):
        public_cpu["huge-pages"] = huge_pages
    max_threads_hint = cpu.get("max-threads-hint")
    if isinstance(max_threads_hint, int) and not isinstance(max_threads_hint, bool):
        public_cpu["max-threads-hint"] = max_threads_hint
    if profile is not None and exact_threads is not None:
        public_cpu[profile] = [-1] * exact_threads

    public = {
        "algo": algorithm,
        "cpu": public_cpu,
        "http": public_http(runtime.get("http")),
        "pools": [public_pool(pool) for pool in pools],
        "randomx": {
            "huge-pages": huge_pages if isinstance(huge_pages, bool) else False,
        },
        "rigos-public-view": {
            "schema": "rigos.xmrig-public-config/v1",
            "identity_redacted": True,
            "construction": "allowlist",
            "source_revision": current.name,
        },
    }
    if exact_threads is not None:
        public["threads"] = exact_threads
""",
    )

    replace_once(
        path,
        '        "identity_redacted_public_view": True,\n',
        '        "identity_redacted_public_view": True,\n        "public_view_construction": "allowlist",\n',
    )


def patch_runtime_fixture() -> None:
    path = ROOT / "scripts/check-alpha8-runtime.py"
    replace_once(
        path,
        '                "max-threads-hint": 2,\n',
        '                "max-threads-hint": 2,\n                "future-secret": "fixture-cpu-secret",\n',
    )
    replace_once(
        path,
        '                    "pass": "fixture-worker",\n',
        '                    "pass": "fixture-worker",\n                    "future-secret": "fixture-pool-secret",\n',
    )
    replace_once(
        path,
        '                "access-token": "fixture-private-token",\n',
        '                "access-token": "fixture-private-token",\n                "future-secret": "fixture-http-secret",\n',
    )
    replace_once(
        path,
        '            "http": {\n',
        '            "future-secret": "fixture-top-secret",\n            "http": {\n',
    )
    replace_once(
        path,
        '    assert public["rigos-public-view"]["identity_redacted"] is True\n',
        '    assert public["rigos-public-view"]["identity_redacted"] is True\n    assert public["rigos-public-view"]["construction"] == "allowlist"\n    assert set(public) == {"algo", "cpu", "http", "pools", "randomx", "rigos-public-view", "threads"}\n    assert set(public["pools"][0]) <= {"url", "algo", "tls", "nicehash", "keepalive", "priority"}\n    assert set(public["http"]) <= {"enabled", "host", "port", "restricted"}\n',
    )
    replace_once(
        path,
        '    assert "fixture-private-token" not in public_text\n',
        '    assert "fixture-private-token" not in public_text\n    assert "fixture-cpu-secret" not in public_text\n    assert "fixture-pool-secret" not in public_text\n    assert "fixture-http-secret" not in public_text\n    assert "fixture-top-secret" not in public_text\n',
    )
    replace_once(
        path,
        '    assert status["profile"] == "rx"\n',
        '    assert status["profile"] == "rx"\n    assert status["public_view_construction"] == "allowlist"\n',
    )


def main() -> int:
    patch_xmrig_backend()
    patch_runtime_renderer()
    patch_runtime_fixture()
    print("Hive Exit hostile review fixes applied")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
