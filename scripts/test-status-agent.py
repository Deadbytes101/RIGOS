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
        with mock.patch.object(agent, "build_observation", return_value=observation), \
             mock.patch.object(agent, "read_secret", return_value="a" * 64), \
             mock.patch.object(agent, "write_status"), \
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
            self.assertEqual(evidence["facts"]["verificationOutcome"], "verified")

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
            self.assertEqual(evidence["facts"]["stateOutcome"], "ready")

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

        encoded = json.dumps({"health": health, "component": component}).lower()
        for forbidden in (
            "secret-wallet",
            "pool.example",
            "hashrate",
            "private-node",
            "accepted_shares",
        ):
            self.assertNotIn(forbidden, encoded)

        self.assertEqual(health["summary"], "rig health completed successfully")
        self.assertEqual(component[0], "operational")

    def test_live_overlay_root_is_operational(self):
        status, authority = agent.root_filesystem_authority(
            "overlay",
            "rw,relatime,lowerdir=/run/live/rootfs/filesystem.squashfs,"
            "upperdir=/run/live/overlay/rw,workdir=/run/live/overlay/work",
        )
        self.assertEqual(status, "operational")
        self.assertEqual(authority["facts"]["rootMode"], "live-overlay")
        self.assertTrue(authority["facts"]["immutableLowerLayer"])
        self.assertFalse(authority["facts"]["rootReadOnly"])

    def test_read_only_root_is_operational(self):
        status, authority = agent.root_filesystem_authority("squashfs", "ro,relatime")
        self.assertEqual(status, "operational")
        self.assertEqual(authority["facts"]["rootMode"], "read-only")

    def test_unexpected_writable_roots_are_major_outage(self):
        status, authority = agent.root_filesystem_authority("ext4", "rw,relatime")
        self.assertEqual(status, "major_outage")
        self.assertEqual(authority["facts"]["rootMode"], "writable")

        status, authority = agent.root_filesystem_authority(
            "overlay",
            "rw,lowerdir=/tmp/untrusted,upperdir=/tmp/upper,workdir=/tmp/work",
        )
        self.assertEqual(status, "major_outage")
        self.assertEqual(authority["facts"]["rootMode"], "unexpected-overlay")

    def test_normal_kernel_boot_messages_do_not_degrade_status(self):
        normal = "\n".join((
            "NMI watchdog: Enabled. Permanently consumes one hw-PMU counter.",
            "thermal_sys: Registered thermal governor 'fair_share'",
            "thermal_sys: Registered thermal governor 'bang_bang'",
            "thermal thermal_zone0: registered",
            "HEST: Hardware Error Source Table (HEST) is initialized",
            "rcu: Preemptible hierarchical RCU implementation.",
            "clocksource: timekeeping watchdog on CPU0: Marking clocksource stable.",
        ))
        self.assertEqual(agent.kernel_fault_counts(normal), (0, 0))

    def test_real_kernel_fault_messages_are_counted_once_per_line(self):
        faulty = "\n".join((
            "mce: [Hardware Error]: Machine check events logged",
            "watchdog: BUG: soft lockup - CPU#0 stuck for 22s!",
            "CPU0: Core temperature above threshold, cpu clock throttled",
            "rcu: INFO: rcu_preempt detected stalls on CPUs/tasks",
        ))
        self.assertEqual(agent.kernel_fault_counts(faulty), (2, 2))

    def test_time_sync_contract_uses_timedatectl_and_service_state(self):
        with mock.patch.object(agent, "run_command", return_value=(0, "yes\n", "")), \
             mock.patch.object(agent, "systemd_show", return_value={"ActiveState": "active"}):
            status, authority = agent.time_synchronization_authority()
        self.assertEqual(status, "operational")
        self.assertTrue(authority["facts"]["ntpSynchronized"])
        self.assertEqual(authority["facts"]["timeServiceActiveState"], "active")


if __name__ == "__main__":
    unittest.main()
