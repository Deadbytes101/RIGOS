#!/usr/bin/env python3
import importlib.machinery
import importlib.util
import json
import os
import stat
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME_RENDER = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render"
MINER_HEALTH = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health"


def load_source(name: str, path: Path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    if spec is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class Environment:
    def __init__(self, **values: str):
        self.values = values
        self.original: dict[str, str | None] = {}

    def __enter__(self):
        for key, value in self.values.items():
            self.original[key] = os.environ.get(key)
            os.environ[key] = value
        return self

    def __exit__(self, _type, _value, _traceback):
        for key, value in self.original.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


class RuntimeRenderApiTests(unittest.TestCase):
    def test_runtime_api_is_local_authenticated_redacted_and_idempotent(self):
        with tempfile.TemporaryDirectory(prefix="rigos-render-api-") as temporary:
            root = Path(temporary)
            state = root / "state"
            runtime = root / "run"
            revision = state / "revisions/r1"
            (revision / "flight-sheets").mkdir(parents=True)
            (revision / "policy.json").write_text(
                json.dumps({
                    "schema": "rigos.policy/v1",
                    "active_flight_sheet": "sheet",
                }),
                encoding="utf-8",
            )
            (revision / "flight-sheets/sheet.json").write_text(
                json.dumps({
                    "schema": "rigos.flight-sheet/v1",
                    "backend": "xmrig",
                    "algorithm": "rx/0",
                    "cpu": {"threads": "auto"},
                }),
                encoding="utf-8",
            )
            (revision / "xmrig.json").write_text(
                json.dumps({
                    "autosave": False,
                    "cpu": {"enabled": True, "huge-pages": True},
                    "pools": [{
                        "url": "pool.example:1234",
                        "user": "secret-wallet",
                        "pass": "worker",
                        "algo": "rx/0",
                    }],
                    "http": {
                        "enabled": True,
                        "host": "0.0.0.0",
                        "port": 9999,
                        "access-token": "attacker-controlled",
                        "restricted": False,
                    },
                }),
                encoding="utf-8",
            )
            state.mkdir(exist_ok=True)
            (state / "current").symlink_to("revisions/r1")

            with Environment(
                RIGOS_STATE_PATH=str(state),
                RIGOS_RUNTIME_PATH=str(runtime),
                RIGOS_RENDER_SKIP_CHOWN="1",
            ):
                module = load_source("rigos_runtime_render_test", RUNTIME_RENDER)
                private_first, public_first, status_first = module.render()
                token_path = runtime / "xmrig-api-token"
                token_first = token_path.read_text(encoding="ascii").strip()

                self.assertEqual(private_first["http"]["host"], "127.0.0.1")
                self.assertEqual(private_first["http"]["port"], 18080)
                self.assertTrue(private_first["http"]["enabled"])
                self.assertTrue(private_first["http"]["restricted"])
                self.assertEqual(private_first["http"]["access-token"], token_first)
                self.assertNotIn("access-token", public_first["http"])
                self.assertNotIn("secret-wallet", json.dumps(public_first))
                self.assertEqual(status_first["http_api"]["host"], "127.0.0.1")
                self.assertEqual(status_first["http_api"]["port"], 18080)
                self.assertNotIn(token_first, json.dumps(status_first))
                self.assertEqual(stat.S_IMODE(token_path.stat().st_mode), 0o600)

                private_second, _, _ = module.render()
                self.assertEqual(private_second["http"]["access-token"], token_first)
                self.assertEqual(token_path.read_text(encoding="ascii").strip(), token_first)

                self.assertEqual(module.main(), 0)
                self.assertEqual(stat.S_IMODE((runtime / "xmrig.json").stat().st_mode), 0o640)
                self.assertEqual(stat.S_IMODE((runtime / "xmrig-public.json").stat().st_mode), 0o644)
                persisted_public = json.loads((runtime / "xmrig-public.json").read_text(encoding="utf-8"))
                self.assertNotIn("access-token", persisted_public["http"])


class SummaryHandler(BaseHTTPRequestHandler):
    expected_token = ""
    summary: dict = {}

    def do_GET(self):
        if self.path != "/2/summary":
            self.send_response(404)
            self.end_headers()
            return
        if self.headers.get("Authorization") != f"Bearer {self.expected_token}":
            self.send_response(401)
            self.end_headers()
            return
        payload = json.dumps(self.summary).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, _format, *_args):
        return


class MinerHealthApiTests(unittest.TestCase):
    def test_authenticated_summary_drives_health_truth(self):
        with tempfile.TemporaryDirectory(prefix="rigos-health-api-") as temporary:
            root = Path(temporary)
            token = "A" * 48
            token_path = root / "xmrig-api-token"
            token_path.write_text(token + "\n", encoding="ascii")
            token_path.chmod(0o600)

            SummaryHandler.expected_token = token
            SummaryHandler.summary = {
                "uptime": 600,
                "algo": "rx/0",
                "hashrate": {"total": [341.2, 340.9, 340.7], "highest": 342.1},
                "connection": {
                    "pool": "pool.example:1234",
                    "uptime": 590,
                    "accepted": 43,
                    "rejected": 0,
                    "failures": 1,
                    "ping": 109,
                },
                "hugepages": [1168, 1168],
            }
            server = HTTPServer(("127.0.0.1", 0), SummaryHandler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                with Environment(
                    RIGOS_XMRIG_API_TOKEN_PATH=str(token_path),
                    RIGOS_XMRIG_API_HOST="127.0.0.1",
                    RIGOS_XMRIG_API_PORT=str(server.server_port),
                    RIGOS_MINER_HEALTH_TEST_MODE="1",
                ):
                    module = load_source("rigos_miner_health_test", MINER_HEALTH)
                    summary, error = module.fetch_api_summary()
                    self.assertIsNone(error)
                    self.assertIsNotNone(summary)
                    metrics = module.summary_metrics(summary)
                    self.assertEqual(metrics["hashrate_60s"], 340.9)
                    self.assertEqual(metrics["accepted_shares"], 43)
                    self.assertEqual(metrics["rejected_shares"], 0)
                    self.assertTrue(metrics["pool_connected"])
                    self.assertEqual(metrics["hugepages_used"], 1168)
                    self.assertEqual(metrics["hugepages_total"], 1168)

                    state, reason = module.classify(
                        {"ActiveState": "active", "SubState": "running", "MainPID": "123"},
                        "S",
                        600,
                        "r1",
                        "ready",
                        "r1",
                        metrics,
                        None,
                        "",
                        True,
                    )
                    self.assertEqual((state, reason), ("ready", None))

                    no_pool = dict(metrics)
                    no_pool.update({
                        "hashrate_10s": 0,
                        "hashrate_60s": 0,
                        "hashrate_15m": 0,
                        "pool_connected": False,
                        "pool": None,
                    })
                    state, reason = module.classify(
                        {"ActiveState": "active", "SubState": "running", "MainPID": "123"},
                        "S",
                        600,
                        "r1",
                        "ready",
                        "r1",
                        no_pool,
                        None,
                        "",
                        True,
                    )
                    self.assertEqual((state, reason), ("waiting_external", "pool_or_network_unavailable"))

                    state, reason = module.classify(
                        {"ActiveState": "active", "SubState": "running", "MainPID": "123"},
                        "S",
                        600,
                        "r1",
                        "ready",
                        "r1",
                        None,
                        "api_unavailable",
                        "cpu accepted (43/0) diff 10000",
                        True,
                    )
                    self.assertEqual((state, reason), ("ready", None))

                    state, reason = module.classify(
                        {"ActiveState": "active", "SubState": "running", "MainPID": "123"},
                        "S",
                        600,
                        "r1",
                        "ready",
                        "r1",
                        None,
                        "api_unavailable",
                        "",
                        True,
                    )
                    self.assertEqual((state, reason), ("degraded", "api_unavailable"))
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
