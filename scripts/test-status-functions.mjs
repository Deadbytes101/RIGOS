import assert from "node:assert/strict";
import test from "node:test";

import {
  COMPONENT_IDS,
  PUBLIC_NODE_LIMIT,
  acceptObservation,
  readPublicStatus,
  validateAndSanitizeObservation,
  verifySignature,
} from "../functions/_lib/status.js";

const SECRET = "a".repeat(64);
const SOURCE_ID = "b".repeat(64);
const OTHER_SOURCE_ID = "e".repeat(64);
const OTHER_SECRET = "f".repeat(64);
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

function observation(sourceId = SOURCE_ID) {
  return {
    schema: "rigos.status-observation/v1",
    observedAt: OBSERVED_AT,
    sourceId,
    bootIdHash: "c".repeat(64),
    release: {
      product: "RIGOS",
      version: "0.0.4-alpha.26",
      imageId: "rigos-usb-amd64",
      imageVersion: "0.0.4-alpha.26",
      channel: "alpha",
      buildId: "20260715.26",
      buildCommit: "85075b271ae5029e887790ec7c8eaa23e8d1b2b8",
      architecture: "x86_64",
    },
    health: {
      status: "ok",
      exitCode: 0,
      summary: "arbitrary sender summary",
    },
    components: COMPONENT_IDS.map((id) => ({
      id,
      status: "operational",
      observedAt: OBSERVED_AT,
      evidence: evidenceFor(id),
    })),
  };
}

async function sign(body, timestamp = NOW, nonce = "d".repeat(32), secret = SECRET) {
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const bodyBytes = encoder.encode(body);
  const prefix = encoder.encode(`${timestamp}.${nonce}.`);
  const canonical = new Uint8Array(prefix.length + bodyBytes.length);
  canonical.set(prefix, 0);
  canonical.set(bodyBytes, prefix.length);
  const digest = new Uint8Array(await crypto.subtle.sign("HMAC", key, canonical));
  const hex = [...digest].map((value) => value.toString(16).padStart(2, "0")).join("");
  return { timestamp, nonce, signature: `sha256=${hex}` };
}

async function signedRequest(value, overrides = {}) {
  const body = JSON.stringify(value);
  const signed = await sign(
    body,
    overrides.timestamp ?? NOW,
    overrides.nonce ?? "d".repeat(32),
    overrides.secret ?? SECRET,
  );
  return new Request("https://rigos.site/api/v1/observations", {
    method: "POST",
    headers: {
      "content-type": overrides.contentType ?? "application/json",
      "x-rigos-timestamp": String(signed.timestamp),
      "x-rigos-nonce": signed.nonce,
      "x-rigos-signature": signed.signature,
    },
    body,
  });
}

function singleSourceEnv(db, sourceId = SOURCE_ID, secret = SECRET) {
  return {
    RIGOS_STATUS_DB: db,
    RIGOS_STATUS_SOURCE_ID: sourceId,
    RIGOS_STATUS_SECRET: secret,
  };
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
      const rows = [...this.db.rows.values()]
        .sort((left, right) => right.received_unix - left.received_unix)
        .slice(0, PUBLIC_NODE_LIMIT);
      return { results: rows };
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

test("exact 19-component observation validates with component fact allowlists", () => {
  const result = validateAndSanitizeObservation(observation(), NOW);
  assert.equal(result.observation.components.length, 19);
  assert.equal(result.observation.release.product, "RIGOS");
  assert.equal(result.observation.sourceId, SOURCE_ID);
  assert.equal(result.observation.health.summary, "rig health completed successfully");
});

test("sender-controlled summaries are replaced with bounded server summaries", () => {
  const value = observation();
  value.components[0].evidence.summary = "wallet 4A-private-value";
  value.health.summary = "pool and password should never be public";
  const result = validateAndSanitizeObservation(value, NOW);
  const serialized = JSON.stringify(result.observation);
  assert.equal(serialized.includes("4A-private-value"), false);
  assert.equal(serialized.includes("password should never be public"), false);
  assert.equal(result.observation.components[0].evidence.summary, "Boot-device verification: verified");
});

test("private-field key variants are rejected before storage", () => {
  for (const key of ["workerName", "walletAddress", "poolAddress", "authToken", "miningIdentity", "ipAddress"]) {
    const value = observation();
    value[key] = "private";
    assert.throws(
      () => validateAndSanitizeObservation(value, NOW),
      (error) => error.code === "private_field_rejected" && error.status === 422,
      key,
    );
  }
});

test("unsupported component facts are rejected instead of copied", () => {
  const value = observation();
  value.components[0].evidence.facts.walletAddress = "private";
  assert.throws(
    () => validateAndSanitizeObservation(value, NOW),
    (error) => error.code === "private_field_rejected" && error.status === 422,
  );

  const other = observation();
  other.components[0].evidence.facts.randomField = "value";
  assert.throws(
    () => validateAndSanitizeObservation(other, NOW),
    (error) => error.code === "invalid_observation" && error.status === 422,
  );
});

test("signed request matches the agent timestamp.nonce.body contract", async () => {
  const value = observation();
  const body = JSON.stringify(value);
  const request = await signedRequest(value);
  const result = await verifySignature(
    request,
    SECRET,
    new TextEncoder().encode(body),
    NOW,
  );
  assert.equal(result.timestamp, NOW);
  assert.equal(result.nonce, "d".repeat(32));
});

test("content type accepts application/json parameters and rejects lookalikes", async () => {
  const db = new MockDatabase();
  const env = singleSourceEnv(db);
  const accepted = await acceptObservation(
    await signedRequest(observation(), { contentType: "application/json; charset=utf-8" }),
    env,
    NOW,
  );
  assert.equal(accepted.status, 202);

  const badRequest = await signedRequest(observation(), {
    contentType: "application/jsonp",
    nonce: "1".repeat(32),
  });
  await assert.rejects(
    () => acceptObservation(badRequest, env, NOW),
    (error) => error.code === "unsupported_media_type" && error.status === 415,
  );
});

test("single-source secret is bound to one source ID", async () => {
  const db = new MockDatabase();
  const env = singleSourceEnv(db);
  const request = await signedRequest(observation(OTHER_SOURCE_ID));
  await assert.rejects(
    () => acceptObservation(request, env, NOW),
    (error) => error.code === "unknown_source" && error.status === 401,
  );
  assert.equal(db.rows.size, 0);
});

test("multi-source registry accepts only the secret assigned to each source", async () => {
  const db = new MockDatabase();
  const env = {
    RIGOS_STATUS_DB: db,
    RIGOS_STATUS_SOURCE_KEYS: JSON.stringify({
      [SOURCE_ID]: SECRET,
      [OTHER_SOURCE_ID]: OTHER_SECRET,
    }),
  };

  const accepted = await acceptObservation(
    await signedRequest(observation(OTHER_SOURCE_ID), {
      secret: OTHER_SECRET,
      nonce: "2".repeat(32),
    }),
    env,
    NOW,
  );
  assert.equal(accepted.status, 202);

  const wrong = await signedRequest(observation(OTHER_SOURCE_ID), {
    secret: SECRET,
    nonce: "3".repeat(32),
  });
  await assert.rejects(
    () => acceptObservation(wrong, env, NOW),
    (error) => error.code === "signature_mismatch" && error.status === 401,
  );
});

test("valid observation is accepted and public output hides full source ID", async () => {
  const db = new MockDatabase();
  const env = singleSourceEnv(db);
  const response = await acceptObservation(await signedRequest(observation()), env, NOW);
  assert.equal(response.status, 202);
  const accepted = await response.json();
  assert.equal(accepted.accepted, true);
  assert.equal(accepted.source, SOURCE_ID.slice(0, 12));

  const status = await readPublicStatus(env, NOW + 30);
  assert.equal(status.nodeCount, 1);
  assert.equal(status.totalNodeCount, 1);
  assert.equal(status.truncated, false);
  assert.equal(status.nodes[0].nodeId, SOURCE_ID.slice(0, 12));
  assert.equal(status.nodes[0].connection, "live");
  assert.equal("sourceId" in status.nodes[0], false);
  assert.equal(JSON.stringify(status).includes(SOURCE_ID), false);
});

test("replayed nonce is rejected with HTTP 409", async () => {
  const db = new MockDatabase();
  const env = singleSourceEnv(db);
  await acceptObservation(await signedRequest(observation()), env, NOW);
  const replayRequest = await signedRequest(observation());
  await assert.rejects(
    () => acceptObservation(replayRequest, env, NOW),
    (error) => error.code === "replay_detected" && error.status === 409,
  );
});

test("wrong secret is rejected before database mutation", async () => {
  const db = new MockDatabase();
  const env = singleSourceEnv(db);
  const invalidRequest = await signedRequest(
    observation(),
    { secret: OTHER_SECRET },
  );
  await assert.rejects(
    () => acceptObservation(invalidRequest, env, NOW),
    (error) => error.code === "signature_mismatch" && error.status === 401,
  );
  assert.equal(db.rows.size, 0);
});

test("freshness changes connection without rewriting last observed system state", async () => {
  const db = new MockDatabase();
  const env = singleSourceEnv(db);
  await acceptObservation(await signedRequest(observation()), env, NOW);

  const stale = (await readPublicStatus(env, NOW + 91)).nodes[0];
  assert.equal(stale.connection, "stale");
  assert.equal(stale.systemState, "operational");

  const offline = (await readPublicStatus(env, NOW + 301)).nodes[0];
  assert.equal(offline.connection, "offline");
  assert.equal(offline.systemState, "operational");
  assert.equal(offline.componentState, "operational");
});

test("public projection reports truncation instead of silently claiming all nodes", async () => {
  const db = new MockDatabase();
  for (let index = 0; index < 40; index += 1) {
    const sourceId = index.toString(16).padStart(64, "0");
    const value = observation(sourceId);
    db.rows.set(sourceId, {
      source_id: sourceId,
      observed_at: value.observedAt,
      observed_unix: NOW + index,
      received_at: new Date((NOW + index) * 1000).toISOString().replace(".000Z", "Z"),
      received_unix: NOW + index,
      release_version: value.release.version,
      build_commit: value.release.buildCommit,
      overall_status: "operational",
      payload_json: JSON.stringify(value),
    });
  }

  const status = await readPublicStatus({ RIGOS_STATUS_DB: db }, NOW + 50);
  assert.equal(status.nodeCount, PUBLIC_NODE_LIMIT);
  assert.equal(status.totalNodeCount, 40);
  assert.equal(status.truncated, true);
});
