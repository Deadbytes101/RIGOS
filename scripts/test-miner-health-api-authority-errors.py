#!/usr/bin/env python3
import importlib.machinery
import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MINER_HEALTH = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health"
PROPERTIES = {"ActiveState": "active", "SubState": "running", "MainPID": "123"}


def load_source(name: str, path: Path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    if spec is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def classify(module, api_error: str | None, journal: str, journal_available: bool = True):
    return module.classify(
        PROPERTIES,
        "S",
        600,
        "r1",
        "ready",
        "r1",
        None,
        api_error,
        journal,
        journal_available,
        {"schema": "rigos.miner-supervisor-state/v1"},
    )


class MinerHealthApiAuthorityErrorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_source("rigos_miner_health_api_authority_errors", MINER_HEALTH)

    def test_missing_token_cannot_be_hidden_by_ready_journal(self):
        state = classify(
            self.module,
            "api_token_missing",
            "miner    speed 10s/60s/15m 341.2 340.9 340.7 H/s",
        )
        self.assertEqual(state, ("degraded", "api_token_missing"))

    def test_authentication_failure_cannot_be_hidden_by_accepted_share(self):
        state = classify(
            self.module,
            "api_http_status_401",
            "cpu accepted (43/0) diff 10000",
        )
        self.assertEqual(state, ("degraded", "api_http_status_401"))

    def test_invalid_api_response_cannot_be_hidden_by_journal(self):
        state = classify(
            self.module,
            "api_response_not_object",
            "cpu accepted (43/0) diff 10000",
        )
        self.assertEqual(state, ("degraded", "api_response_not_object"))

    def test_transient_api_unavailable_still_allows_journal_fallback(self):
        state = classify(
            self.module,
            "api_unavailable",
            "cpu accepted (43/0) diff 10000",
        )
        self.assertEqual(state, ("ready", None))

    def test_transient_api_unavailable_without_journal_is_unknown(self):
        state = classify(self.module, "api_unavailable", "", journal_available=False)
        self.assertEqual(state, ("unknown", "api_unavailable"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
