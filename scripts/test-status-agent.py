#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import hmac
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
AGENT_PATH = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-status-agent"
LOADER = importlib.machinery.SourceFileLoader("rigos_status_agent", str(AGENT_PATH))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
assert SPEC is not None
agent = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(agent)


class AgentTests(unittest.TestCase):
    def test_registry_is_exact_and_private_values_are_absent(self):
        observed = "2026-07-15T00:00:00Z"
        components = [
            agent.component(cid, "unknown", observed, agent.evidence("unavailable"))
            for cid in agent.COMPONENT_IDS
        ]
        with tempfile.TemporaryDirectory() as directory, \
             mock.patch.object(agent, "utc_now", return_value=observed), \
             mock.patch.object(agent, "collect_components", return_value=(
                 {"status": "unavailable", "exitCode": None, "summary": "unavailable"},
                 components,
             )), \
             mock.patch.object(agent, "release_identity", return_value={
                 "product": "RIGOS", "version": "0.0.4-alpha.26", "imageId": "rigos-usb-amd64",
                 "imageVersion": "0.0.4-alpha.26", "channel": "alpha", "buildId": None,
                 "buildCommit": "a" * 40, "architecture": "x86_64",
             }), \
             mock.patch.object(agent, "boot_id_hash", return_value="b" * 64), \
             mock.patch.object(agent, "ensure_source_id", return_value="c" * 64):
            value = agent.build_observation(Path(directory) / "source-id")
        self.assertEqual(len(value["components"]), 19)
        self.assertEqual({item["id"] for item in value["components"]}, set(agent.COMPONENT_IDS))
        encoded = json.dumps(value).lower()
        for forbidden in (
            "wallet", "worker", "hashrate", "hostname", "password", "api token",
            "private key", "pool address", "accepted shares", "rejected shares",
        ):
            self.assertNotIn(forbidden, encoded)

    def test_hmac_wire_contract_is_unchanged(self):
        secret = "a" * 64
        body = b'{"schema":"rigos.status-observation/v1"}'
        with mock.patch.object(agent.secrets, "token_hex", return_value="b" * 32):
            headers = agent.signed_headers(secret, body, now=1784073600)
        canonical = b"1784073600." + (b"b" * 32) + b"." + body
        expected = hmac.new(secret.encode(), canonical, hashlib.sha256).hexdigest()
        self.assertEqual(headers["X-RigOS-Signature"], "sha256=" + expected)

    def test_secret_is_exact_hex(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "secret"
            path.write_text("a" * 64 + "\n", encoding="ascii")
            self.assertEqual(agent.read_secret(path), "a" * 64)
            for invalid in ("s" * 64, "a" * 63, "a" * 65, ""):
                path.write_text(invalid, encoding="ascii")
                with self.assertRaises(RuntimeError):
                    agent.read_secret(path)

    def test_endpoint_rejects_credentials_query_and_fragment(self):
        self.assertEqual(
            agent.endpoint_url("https://status.example/"),
            "https://status.example/api/v1/observations",
        )
        for invalid in (
            "https://user:pass@status.example",
            "https://status.example/?token=x",
            "https://status.example/#x",
            "file:///tmp/status",
        ):
            with self.assertRaises(RuntimeError):
                agent.endpoint_url(invalid)

    def test_observer_failure_is_excluded_from_failed_unit_truth(self):
        units, ignored = agent.unexpected_failed_units(
            "rigos-status-agent.service loaded failed failed observer\n"
            "rigos-state.service loaded failed failed state\n"
        )
        self.assertEqual(units, ["rigos-state.service"])
        self.assertEqual(ignored, 1)

    def test_source_identity_refuses_nonpersistent_state(self):
        with tempfile.TemporaryDirectory() as directory, \
             mock.patch.object(agent, "STATE_ROOT", Path(directory) / "state"), \
             mock.patch.object(agent.os.path, "ismount", return_value=False):
            (Path(directory) / "state").mkdir()
            with self.assertRaises(RuntimeError):
                agent.ensure_source_id(Path(directory) / "state/status-agent/source-id")

    def test_transport_and_protocol_failures_are_bounded_exit_codes(self):
        observation = {"observedAt": "2026-07-15T00:00:00Z"}
        common = [
            mock.patch.object(agent, "build_observation", return_value=observation),
            mock.patch.object(agent, "read_secret", return_value="a" * 64),
            mock.patch.object(agent, "write_status"),
        ]
        with common[0], common[1], common[2], \
             mock.patch.object(agent, "send_observation", side_effect=agent.TransportError("offline")):
            self.assertEqual(agent.main(["--server", "https://status.example"]), 75)
        with mock.patch.object(agent, "build_observation", return_value=observation), \
             mock.patch.object(agent, "read_secret", return_value="a" * 64), \
             mock.patch.object(agent, "write_status"), \
             mock.patch.object(agent, "send_observation", side_effect=agent.ProtocolError("rejected")):
            self.assertEqual(agent.main(["--server", "https://status.example"]), 76)


    def test_authority_specific_outcome_fields_are_honored(self):
        with tempfile.TemporaryDirectory() as directory:
            boot = Path(directory) / "boot.json"
            boot.write_text(json.dumps({
                "schema": "rigos.boot-device/v1",
                "verification_outcome": "verified",
            }), encoding="utf-8")

            status, evidence = agent.json_authority(
                str(boot),
                "verificationOutcome",
                "Boot",
            )

            self.assertEqual(status, "operational")
            self.assertEqual(
                evidence["facts"]["verificationOutcome"],
                "verified",
            )

            recovery = Path(directory) / "recovery.json"
            recovery.write_text(json.dumps({
                "schema": "rigos.recovery-access/v1",
                "state_outcome": "ready",
            }), encoding="utf-8")

            status, evidence = agent.json_authority(
                str(recovery),
                "stateOutcome",
                "Recovery",
            )

            self.assertEqual(status, "operational")
            self.assertEqual(
                evidence["facts"]["stateOutcome"],
                "ready",
            )

    def test_operator_health_never_forwards_command_output(self):
        private_output = json.dumps({
            "wallet": "secret-wallet",
            "pool": "pool.example:443",
            "hashrate": 123.4,
            "identity": "private-node",
            "accepted_shares": 9,
        })

        with mock.patch.object(
            agent,
            "run_command",
            return_value=(0, private_output, ""),
        ):
            health, component = agent.operator_health()

        encoded = json.dumps({
            "health": health,
            "component": component,
        }).lower()

        for forbidden in (
            "secret-wallet",
            "pool.example",
            "hashrate",
            "private-node",
            "accepted_shares",
        ):
            self.assertNotIn(forbidden, encoded)

        self.assertEqual(
            health["summary"],
            "rig health completed successfully",
        )
        self.assertEqual(component[0], "operational")


if __name__ == "__main__":
    unittest.main()
