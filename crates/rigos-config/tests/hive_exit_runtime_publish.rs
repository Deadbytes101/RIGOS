use serde_json::Value;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn write_json(path: &Path, value: Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn staged_runtime_publication_is_allowlisted_atomic_and_fail_closed() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-runtime-publish-{unique}"));
    let state = root.join("state");
    let runtime = root.join("run");
    let revision = state.join("revisions/r1");
    fs::create_dir_all(revision.join("flight-sheets")).unwrap();
    fs::create_dir_all(&runtime).unwrap();
    symlink("revisions/r1", state.join("current")).unwrap();

    write_json(
        &revision.join("policy.json"),
        serde_json::json!({
            "schema": "rigos.policy/v1",
            "active_flight_sheet": "xmr"
        }),
    );
    write_json(
        &revision.join("flight-sheets/xmr.json"),
        serde_json::json!({
            "schema": "rigos.flight-sheet/v1",
            "backend": "xmrig",
            "algorithm": "rx/0",
            "cpu": {
                "threads": 2,
                "huge_pages": true,
                "max_threads_hint": 100
            }
        }),
    );
    write_json(
        &revision.join("xmrig.json"),
        serde_json::json!({
            "future_top": "TOP_SENTINEL",
            "cpu": {
                "enabled": true,
                "huge-pages": true,
                "max-threads-hint": 2,
                "future_cpu": "CPU_SENTINEL"
            },
            "pools": [{
                "url": "identity:worker@pool.test:1",
                "algo": "rx/0",
                "user": "IDENTITY_SENTINEL",
                "pass": "WORKER_SENTINEL",
                "future_pool": "POOL_SENTINEL"
            }],
            "http": {
                "enabled": false,
                "access-token": "TOKEN_SENTINEL",
                "future_http": "HTTP_SENTINEL"
            }
        }),
    );

    let renderer = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render");
    let publisher = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-publish");
    let gate = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-gate");
    let renderer_wrapper = root.join("renderer");
    fs::write(
        &renderer_wrapper,
        format!("#!/bin/sh\nexec python3 '{}'\n", renderer.display()),
    )
    .unwrap();
    fs::set_permissions(&renderer_wrapper, fs::Permissions::from_mode(0o755)).unwrap();

    let jq = Command::new("jq").arg("--version").status().unwrap();
    assert!(
        jq.success(),
        "jq is required by runtime publication authority"
    );

    let result = Command::new("/bin/sh")
        .arg(&publisher)
        .env("RIGOS_STATE_PATH", &state)
        .env("RIGOS_RUNTIME_PATH", &runtime)
        .env("RIGOS_RUNTIME_RENDERER", &renderer_wrapper)
        .env("RIGOS_RENDER_SKIP_CHOWN", "1")
        .status()
        .unwrap();
    assert!(result.success(), "runtime publisher failed");

    let private = read_json(&runtime.join("xmrig.json"));
    assert_eq!(private["future_top"], "TOP_SENTINEL");
    assert_eq!(private["cpu"]["future_cpu"], "CPU_SENTINEL");
    assert_eq!(private["pools"][0]["future_pool"], "POOL_SENTINEL");
    assert_eq!(private["http"]["future_http"], "HTTP_SENTINEL");
    assert_eq!(private["cpu"]["max-threads-hint"], 100);
    assert_eq!(private["cpu"]["rx"], serde_json::json!([-1, -1]));

    let public = read_json(&runtime.join("xmrig-public.json"));
    let public_keys = public
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        public_keys,
        vec![
            "algo",
            "cpu",
            "http",
            "pools",
            "randomx",
            "rigos-public-view",
            "threads"
        ]
    );
    assert_eq!(public["algo"], "rx/0");
    assert_eq!(public["threads"], 2);
    assert_eq!(public["cpu"]["max-threads-hint"], 100);
    assert_eq!(public["cpu"]["rx"], serde_json::json!([-1, -1]));
    assert_eq!(public["randomx"]["huge-pages"].as_bool(), Some(true));
    assert_eq!(public["pools"][0]["url"], "pool.test:1");
    assert_eq!(public["rigos-public-view"]["construction"], "allowlist");
    let public_text = serde_json::to_string(&public).unwrap();
    for sentinel in [
        "TOP_SENTINEL",
        "CPU_SENTINEL",
        "POOL_SENTINEL",
        "HTTP_SENTINEL",
        "IDENTITY_SENTINEL",
        "WORKER_SENTINEL",
        "TOKEN_SENTINEL",
    ] {
        assert!(
            !public_text.contains(sentinel),
            "public view leaked {sentinel}"
        );
    }

    let status = read_json(&runtime.join("runtime-config-status.json"));
    assert_eq!(status["outcome"], "ready");
    assert_eq!(status["revision"], "r1");
    assert_eq!(status["exact_threads"], 2);
    assert_eq!(status["public_view_construction"], "allowlist");

    assert_eq!(
        fs::metadata(runtime.join("xmrig.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    for path in [
        runtime.join("xmrig-public.json"),
        runtime.join("runtime-config-status.json"),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
    let leftovers = fs::read_dir(&runtime)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('.'))
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "temporary runtime files remain: {leftovers:?}"
    );

    let allowed = Command::new("python3")
        .arg(&gate)
        .arg("--state")
        .arg(&state)
        .arg("--runtime")
        .arg(&runtime)
        .status()
        .unwrap();
    assert!(allowed.success());

    let mut invalid = private;
    invalid["cpu"]["rx"] = serde_json::json!([-1]);
    write_json(&runtime.join("xmrig.json"), invalid);
    let denied = Command::new("python3")
        .arg(&gate)
        .arg("--state")
        .arg(&state)
        .arg("--runtime")
        .arg(&runtime)
        .status()
        .unwrap();
    assert_eq!(denied.code(), Some(2));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_jq_is_rejected_before_runtime_staging() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-runtime-missing-jq-{unique}"));
    let runtime = root.join("run");
    fs::create_dir_all(&runtime).unwrap();

    let publisher = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-publish");
    let missing_jq = root.join("missing-jq");
    let output = Command::new("/bin/sh")
        .arg(&publisher)
        .env("RIGOS_RUNTIME_PATH", &runtime)
        .env("RIGOS_JQ", &missing_jq)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("required jq runtime is missing"));
    assert!(stderr.contains(missing_jq.to_str().unwrap()));
    assert_eq!(fs::read_dir(&runtime).unwrap().count(), 0);

    let _ = fs::remove_dir_all(root);
}
