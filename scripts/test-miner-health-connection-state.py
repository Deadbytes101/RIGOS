#!/usr/bin/env python3
import importlib.machinery
import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MINER_HEALTH = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health"


def load_source(name: str, path: Path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    if spec is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def classify(module, metrics):
    return module.classify(
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


class MinerHealthConnectionStateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_source("rigos_miner_health_connection_state", MINER_HEALTH)

    def test_stale_pool_name_after_disconnect_is_waiting_external(self):
        metrics = self.module.summary_metrics({
            "uptime": 900,
            "algo": "rx/0",
            "hashrate": {"total": [0, 0, 0], "highest": 341.2},
            "connection": {
                "pool": "pool.example:1234",
                "ip": None,
                "uptime": 0,
                "uptime_ms": 0,
                "accepted": 43,
                "rejected": 0,
                "failures": 3,
                "ping": 0,
            },
            "hugepages": [1168, 1168],
        })

        self.assertEqual(metrics["pool"], "pool.example:1234")
        self.assertIsNone(metrics["connection_ip"])
        self.assertFalse(metrics["pool_connected"])
        self.assertEqual(
            classify(self.module, metrics),
            ("waiting_external", "pool_or_network_unavailable"),
        )

    def test_active_connection_without_hashrate_is_degraded(self):
        metrics = self.module.summary_metrics({
            "uptime": 900,
            "algo": "rx/0",
            "hashrate": {"total": [0, 0, 0], "highest": 341.2},
            "connection": {
                "pool": "pool.example:1234",
                "ip": "203.0.113.10",
                "uptime": 0,
                "uptime_ms": 125,
                "accepted": 43,
                "rejected": 0,
                "failures": 0,
                "ping": 109,
            },
            "hugepages": [1168, 1168],
        })

        self.assertTrue(metrics["pool_connected"])
        self.assertEqual(metrics["connection_ip"], "203.0.113.10")
        self.assertEqual(metrics["connection_uptime_ms"], 125)
        self.assertEqual(classify(self.module, metrics), ("degraded", "no_hashrate_from_api"))

    def test_seconds_uptime_remains_compatible_without_ip_or_milliseconds(self):
        metrics = self.module.summary_metrics({
            "hashrate": {"total": [0, 0, 0]},
            "connection": {
                "pool": "pool.example:1234",
                "uptime": 5,
            },
        })

        self.assertTrue(metrics["pool_connected"])
        self.assertEqual(metrics["connection_uptime_seconds"], 5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
