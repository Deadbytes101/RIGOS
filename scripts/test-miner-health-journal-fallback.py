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


class MinerHealthJournalFallbackTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_source("rigos_miner_health_journal_fallback", MINER_HEALTH)

    def test_newer_external_error_overrides_old_ready_evidence(self):
        journal = "\n".join([
            "miner    speed 10s/60s/15m 341.2 340.9 340.7 H/s",
            "cpu accepted (43/0) diff 10000",
            "net connect error: operation timed out",
        ])

        self.assertEqual(self.module.latest_journal_signal(journal), "external_wait")
        self.assertEqual(
            self.module.journal_fallback_state(journal, 600, "api_unavailable"),
            ("waiting_external", "pool_or_network_unavailable"),
        )

    def test_newer_ready_evidence_overrides_old_external_error(self):
        journal = "\n".join([
            "net connect error: operation timed out",
            "miner    speed 10s/60s/15m 341.2 340.9 340.7 H/s",
            "cpu accepted (44/0) diff 10000",
        ])

        self.assertEqual(self.module.latest_journal_signal(journal), "ready")
        self.assertEqual(
            self.module.journal_fallback_state(journal, 600, "api_unavailable"),
            ("ready", None),
        )

    def test_no_signal_during_warmup_is_warming_up(self):
        self.assertIsNone(self.module.latest_journal_signal("starting miner"))
        self.assertEqual(
            self.module.journal_fallback_state("starting miner", 30, "api_unavailable"),
            ("warming_up", None),
        )

    def test_no_signal_after_warmup_is_degraded(self):
        self.assertEqual(
            self.module.journal_fallback_state("miner remains silent", 600, "api_unavailable"),
            ("degraded", "api_unavailable"),
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
