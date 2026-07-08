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


class MinerHealthApiSchemaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_source("rigos_miner_health_api_schema", MINER_HEALTH)

    def test_fractional_counters_are_rejected_not_truncated(self):
        metrics = self.module.summary_metrics({
            "hashrate": {"total": [341.2, 340.9, 340.7]},
            "connection": {
                "pool": "pool.example:1234",
                "ip": "203.0.113.10",
                "uptime": 10,
                "accepted": 43.9,
                "rejected": 1.2,
                "failures": 2.5,
                "ping": 109.7,
            },
            "results": {
                "shares_good": 42.8,
                "shares_total": 44.1,
            },
            "hugepages": [1168.5, 1169.5],
        })

        self.assertIsNone(metrics["accepted_shares"])
        self.assertIsNone(metrics["rejected_shares"])
        self.assertIsNone(metrics["connection_failures"])
        self.assertIsNone(metrics["pool_ping_ms"])
        self.assertIsNone(metrics["hugepages_used"])
        self.assertIsNone(metrics["hugepages_total"])

    def test_boolean_counters_are_rejected(self):
        metrics = self.module.summary_metrics({
            "connection": {"accepted": True, "rejected": False},
            "hugepages": [True, False],
        })

        self.assertIsNone(metrics["accepted_shares"])
        self.assertIsNone(metrics["rejected_shares"])
        self.assertIsNone(metrics["hugepages_used"])
        self.assertIsNone(metrics["hugepages_total"])

    def test_integer_result_fallback_remains_supported(self):
        metrics = self.module.summary_metrics({
            "results": {"shares_good": 43, "shares_total": 45},
            "hugepages": [1168, 1168],
            "connection": {"failures": 2, "ping": 109},
        })

        self.assertEqual(metrics["accepted_shares"], 43)
        self.assertEqual(metrics["rejected_shares"], 2)
        self.assertEqual(metrics["connection_failures"], 2)
        self.assertEqual(metrics["pool_ping_ms"], 109)
        self.assertEqual(metrics["hugepages_used"], 1168)
        self.assertEqual(metrics["hugepages_total"], 1168)

    def test_total_below_accepted_does_not_produce_negative_rejects(self):
        metrics = self.module.summary_metrics({
            "results": {"shares_good": 45, "shares_total": 43},
        })

        self.assertEqual(metrics["accepted_shares"], 45)
        self.assertIsNone(metrics["rejected_shares"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
