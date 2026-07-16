import assert from "node:assert/strict";
import test from "node:test";

import {
  COMPONENT_IDS,
  OBSERVATION_SCHEMA,
  readPublicStatus,
} from "../functions/_lib/status.js";

const SOURCE_ID = "b".repeat(64);
const NOW = 1784131200;
const OBSERVED_AT = new Date(NOW * 1000).toISOString().replace(".000Z", "Z");

function evidenceFor(id) {
  const base = {
    authority: "rigos-status-agent",
    summary: `${id} is operational`,
  };

  switch (id) {
    case "boot-device-verification":
      return { ...base, schema: "rigos.boot-device/v1", outcome: "verified", facts: { verificationOutcome: "verified" } };
    case "persistent-state":
      return { ...base, schema: "rigos.state-status/v1", outcome: "ready", facts: { stateOutcome: "ready" } };
    case "state-readiness":
      return { ...base, unit: "rigos-state-ready.service", activeState: "active", subState: "exited", result: "success", facts: { conditionResult: "yes", unitFileState: "enabled" } };
    case "recovery-access":
      return { ...base, schema: "rigos.recovery-access-status/v1", outcome: "ready", facts: { stateOutcome: "ready" } };
    case "ssh-host-identity":
      return { ...base, unit: "rigos-ssh-hostkeys.service", activeState: "active", subState: "exited", result: "success", facts: { conditionResult: "yes", unitFileState: "enabled" } };
    case "configuration-activation":
      return { ...base, schema: "rigos.activation-status/v1", outcome: "ready", facts: { configurationState: "ready" } };
    case "profile-apply":
      return { ...base, unit: "rigos-profile-apply.service", activeState: "active", subState: "exited", result: "success", facts: { conditionResult: "yes", unitFileState: "enabled" } };
    case "runtime-render":
      return { ...base, unit: "rigos-runtime-render.service", activeState: "active", subState: "exited", result: "success", facts: { conditionResult: "yes", unitFileState: "enabled" } };
    case "huge-page-authority":
      return { ...base, schema: "rigos.performance-status/v1", outcome: "ready", facts: { targetPages: 1280, actualPages: 1280, allocationPercent: 100 } };
    case "randomx-msr":
      return {
        ...base,
        unit: "rigos-randomx-msr.service",
        activeState: "active",
        subState: "exited",
        result: "success",
        facts: { conditionResult: "yes", unitFileState: "enabled" },
      };
    case "network-readiness":
      return { ...base, unit: "NetworkManager.service", activeState: "active", subState: "running", result: "success", facts: { conditionResult: "yes", unitFileState: "enabled" } };
    case "root-filesystem-integrity":
      return { ...base, facts: { immutableLowerLayer: true, rootFileSystem: "overlay", rootMode: "read-only", rootReadOnly: true } };
    case "failed-unit-set":
      return { ...base, facts: { failedUnits: 0, ignoredObserverFailures: 0 } };
    case "operator-health":
      return { ...base, facts: { commandExitCode: 0 } };
    case "kernel-integrity":
      return { ...base, facts: { severeEvents: 0, warningEvents: 0 } };
    case "state-capacity":
      return { ...base, facts: { freePercent: 88.25 } };
    case "time-synchronization":
      return { ...base, facts: { ntpSynchronized: true, timeServiceActiveState: "active" } };
    case "ssh-service":
      return { ...base, unit: "ssh.service", activeState: "active", subState: "running", result: "success", facts: { conditionResult: "yes", unitFileState: "enabled" } };
    case "remote-access-observer":
      return { ...base, unit: "rigos-remote-access-observe.service", activeState: "active", subState: "exited", result: "success", facts: { conditionResult: "yes", unitFileState: "static" } };
    default:
      throw new Error(`missing fixture for ${id}`);
  }
}

function observation() {
  return {
    schema: OBSERVATION_SCHEMA,
    observedAt: OBSERVED_AT,
    sourceId: SOURCE_ID,
    bootIdHash: "c".repeat(64),
    release: {
      product: "RIGOS",
      version: "0.0.4-alpha.26",
      imageId: "rigos-usb-amd64",
      imageVersion: "0.0.4-alpha.26",
      channel: "alpha",
      buildId: "20260715.26",
      buildCommit: "3e3440434172ebb68b96cbec8bdd9ef3b649d5af",
      architecture: "x86_64",
    },
    health: {
      status: "ok",
      exitCode: 0,
      summary: "rig health completed successfully",
    },
    components: COMPONENT_IDS.map((id) => ({
      id,
      status: "operational",
      observedAt: OBSERVED_AT,
      evidence: evidenceFor(id),
    })),
  };
}

class Statement {
  constructor(sql, rows) {
    this.sql = sql;
    this.rows = rows;
  }

  async all() {
    if (this.sql.includes("COUNT(*)")) {
      return { results: [{ total: this.rows.length }] };
    }
    return { results: this.rows };
  }
}

class Database {
  constructor(rows) {
    this.rows = rows;
  }

  prepare(sql) {
    return new Statement(sql, this.rows);
  }
}

test("legacy stored payloads are re-sanitized before public projection", async () => {
  const stored = {
    schema: OBSERVATION_SCHEMA,
    observedAt: OBSERVED_AT,
    sourceId: SOURCE_ID,
    workerName: "private-worker-name",
  };
  const db = new Database([{
    source_id: SOURCE_ID,
    received_at: stored.observedAt,
    received_unix: NOW,
    payload_json: JSON.stringify(stored),
  }]);

  const status = await readPublicStatus({ RIGOS_STATUS_DB: db }, NOW + 1);
  const serialized = JSON.stringify(status);

  assert.equal(status.nodeCount, 0);
  assert.equal(status.totalNodeCount, 1);
  assert.equal(status.truncated, true);
  assert.equal(serialized.includes("private-worker-name"), false);
  assert.equal(serialized.includes(SOURCE_ID), false);
});

test("stored RandomX MSR unit evidence remains visible in public status", async () => {
  const stored = observation();
  const db = new Database([{
    source_id: SOURCE_ID,
    received_at: stored.observedAt,
    received_unix: NOW,
    payload_json: JSON.stringify(stored),
  }]);

  const status = await readPublicStatus({ RIGOS_STATUS_DB: db }, NOW + 5);

  assert.equal(status.nodeCount, 1);
  assert.equal(status.totalNodeCount, 1);
  assert.equal(status.truncated, false);
  assert.equal(status.nodes[0].nodeId, SOURCE_ID.slice(0, 12));
  assert.equal(status.nodes[0].connection, "live");
  assert.equal(status.nodes[0].components.length, 19);

  const randomx = status.nodes[0].components.find(({ id }) => id === "randomx-msr");
  assert.deepEqual(randomx.evidence, {
    authority: "rigos-status-agent",
    unit: "rigos-randomx-msr.service",
    activeState: "active",
    subState: "exited",
    result: "success",
    facts: {
      conditionResult: "yes",
      unitFileState: "enabled",
    },
    summary: "RandomX MSR: active/exited",
  });
});
