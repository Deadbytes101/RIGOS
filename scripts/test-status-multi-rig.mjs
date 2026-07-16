import assert from "node:assert/strict";
import test from "node:test";

import {
  COMPONENT_IDS,
  acceptObservation,
  readPublicStatus,
} from "../functions/_lib/status.js";

const SOURCE_A = "a".repeat(64);
const SOURCE_B = "b".repeat(64);
const SECRET_A = "c".repeat(64);
const SECRET_B = "d".repeat(64);
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
      return { ...base, schema: "rigos.randomx-msr-status/v1", outcome: "ready", facts: { verificationOutcome: "ready" } };
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

function observation(sourceId, legacyRandomx = false) {
  const value = {
    schema: "rigos.status-observation/v1",
    observedAt: OBSERVED_AT,
    sourceId,
    bootIdHash: "e".repeat(64),
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

  if (legacyRandomx) {
    const component = value.components.find(({ id }) => id === "randomx-msr");
    component.evidence = {
      authority: "rigos-status-agent",
      summary: "rigos-randomx-msr.service is active/exited",
      unit: "rigos-randomx-msr.service",
      activeState: "active",
      subState: "exited",
      result: "success",
      facts: {
        conditionResult: "yes",
        unitFileState: "enabled",
      },
    };
  }

  return value;
}

async function sign(body, secret, nonce) {
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const bodyBytes = encoder.encode(body);
  const prefix = encoder.encode(`${NOW}.${nonce}.`);
  const canonical = new Uint8Array(prefix.length + bodyBytes.length);
  canonical.set(prefix, 0);
  canonical.set(bodyBytes, prefix.length);
  const digest = new Uint8Array(await crypto.subtle.sign("HMAC", key, canonical));
  const hex = [...digest].map((value) => value.toString(16).padStart(2, "0")).join("");
  return `sha256=${hex}`;
}

async function signedRequest(value, secret, nonce) {
  const body = JSON.stringify(value);
  return new Request("https://rigos.site/api/v1/observations", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-rigos-timestamp": String(NOW),
      "x-rigos-nonce": nonce,
      "x-rigos-signature": await sign(body, secret, nonce),
    },
    body,
  });
}

class MockStatement {
  constructor(db, sql) {
    this.db = db;
    this.sql = sql;
    this.values = [];
  }

  bind(...values) {
    this.values = values;
    return this;
  }

  async all() {
    if (this.sql.includes("COUNT(*)")) {
      return { results: [{ total: this.db.rows.size }] };
    }
    if (this.sql.includes("FROM status_observations")) {
      return {
        results: [...this.db.rows.values()]
          .sort((left, right) => right.received_unix - left.received_unix)
          .slice(0, 32),
      };
    }
    return { results: [] };
  }
}

class MockDatabase {
  constructor() {
    this.nonces = new Set();
    this.rows = new Map();
  }

  prepare(sql) {
    return new MockStatement(this, sql);
  }

  async batch(statements) {
    const nonce = statements[1].values[0];
    if (this.nonces.has(nonce)) {
      throw new Error("UNIQUE constraint failed: ingest_nonces.nonce");
    }

    const [
      sourceId,
      observedAt,
      observedUnix,
      receivedAt,
      receivedUnix,
      releaseVersion,
      buildCommit,
      overallStatus,
      payloadJson,
    ] = statements[2].values;

    this.nonces.add(nonce);
    this.rows.set(sourceId, {
      source_id: sourceId,
      observed_at: observedAt,
      observed_unix: observedUnix,
      received_at: receivedAt,
      received_unix: receivedUnix,
      release_version: releaseVersion,
      build_commit: buildCommit,
      overall_status: overallStatus,
      payload_json: payloadJson,
    });
    return statements.map(() => ({ success: true }));
  }
}

function environment(db) {
  return {
    RIGOS_STATUS_DB: db,
    RIGOS_STATUS_SOURCE_KEYS: JSON.stringify({
      [SOURCE_A]: SECRET_A,
      [SOURCE_B]: SECRET_B,
    }),
  };
}

test("legacy RandomX MSR unit evidence is accepted and bounded", async () => {
  const db = new MockDatabase();
  const response = await acceptObservation(
    await signedRequest(observation(SOURCE_B, true), SECRET_B, "1".repeat(32)),
    environment(db),
    NOW,
  );

  assert.equal(response.status, 202);
  const stored = JSON.parse(db.rows.get(SOURCE_B).payload_json);
  const component = stored.components.find(({ id }) => id === "randomx-msr");
  assert.deepEqual(component.evidence, {
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

test("two independently signed rigs appear in the public projection", async () => {
  const db = new MockDatabase();
  const env = environment(db);

  await acceptObservation(
    await signedRequest(observation(SOURCE_A), SECRET_A, "2".repeat(32)),
    env,
    NOW,
  );
  await acceptObservation(
    await signedRequest(observation(SOURCE_B, true), SECRET_B, "3".repeat(32)),
    env,
    NOW,
  );

  const status = await readPublicStatus(env, NOW + 5);
  assert.equal(status.nodeCount, 2);
  assert.equal(status.totalNodeCount, 2);
  assert.equal(status.truncated, false);
  assert.deepEqual(
    new Set(status.nodes.map(({ nodeId }) => nodeId)),
    new Set([SOURCE_A.slice(0, 12), SOURCE_B.slice(0, 12)]),
  );
  assert.equal(status.nodes.every(({ connection }) => connection === "live"), true);
  assert.equal(status.nodes.every(({ components }) => components.length === 19), true);
});

test("a secret cannot sign for a different registered rig", async () => {
  const db = new MockDatabase();
  await assert.rejects(
    async () => acceptObservation(
      await signedRequest(observation(SOURCE_B, true), SECRET_A, "4".repeat(32)),
      environment(db),
      NOW,
    ),
    (error) => error.code === "signature_mismatch" && error.status === 401,
  );
  assert.equal(db.rows.size, 0);
});
