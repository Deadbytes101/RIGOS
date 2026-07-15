import assert from "node:assert/strict";
import test from "node:test";

import {
  COMPONENT_IDS,
  acceptObservation,
  readPublicStatus,
  validateAndSanitizeObservation,
  verifySignature,
} from "../functions/_lib/status.js";

const SECRET = "a".repeat(64);
const NOW = 1784131200;
const OBSERVED_AT = new Date(NOW * 1000).toISOString().replace(".000Z", "Z");

function factsFor(id) {
  switch (id) {
    case "huge-page-authority":
      return { targetPages: 128, actualPages: 128, allocationPercent: 100 };
    case "kernel-integrity":
      return { severeEvents: 0, warningEvents: 0 };
    case "state-capacity":
      return { freePercent: 88.25 };
    case "time-synchronization":
      return { ntpSynchronized: true };
    case "operator-health":
      return { commandExitCode: 0 };
    default:
      return { unitFileState: "enabled", conditionResult: "yes" };
  }
}

function observation() {
  return {
    schema: "rigos.status-observation/v1",
    observedAt: OBSERVED_AT,
    sourceId: "b".repeat(64),
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
      summary: "rig health completed successfully",
    },
    components: COMPONENT_IDS.map((id) => ({
      id,
      status: "operational",
      observedAt: OBSERVED_AT,
      evidence: {
        authority: "rigos-status-agent",
        summary: `${id} is operational`,
        facts: factsFor(id),
      },
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
      "content-type": "application/json",
      "x-rigos-timestamp": String(signed.timestamp),
      "x-rigos-nonce": signed.nonce,
      "x-rigos-signature": signed.signature,
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
    if (this.sql.includes("FROM status_observations")) {
      return { results: [...this.db.rows.values()] };
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

test("exact 19-component observation validates and sanitizes", () => {
  const result = validateAndSanitizeObservation(observation(), NOW);
  assert.equal(result.observation.components.length, 19);
  assert.equal(result.observation.release.product, "RIGOS");
  assert.equal(result.observation.sourceId, "b".repeat(64));
});

test("private mining keys are rejected before storage", () => {
  const value = observation();
  value.pool = "example.invalid:443";
  assert.throws(
    () => validateAndSanitizeObservation(value, NOW),
    (error) => error.code === "private_field_rejected" && error.status === 422,
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

test("valid signed observation is accepted and exposed without full source ID", async () => {
  const db = new MockDatabase();
  const env = { RIGOS_STATUS_DB: db, RIGOS_STATUS_SECRET: SECRET };
  const response = await acceptObservation(await signedRequest(observation()), env, NOW);
  assert.equal(response.status, 202);
  const accepted = await response.json();
  assert.equal(accepted.accepted, true);
  assert.equal(accepted.source, "b".repeat(12));

  const status = await readPublicStatus(env, NOW + 30);
  assert.equal(status.nodeCount, 1);
  assert.equal(status.nodes[0].nodeId, "b".repeat(12));
  assert.equal(status.nodes[0].connection, "live");
  assert.equal("sourceId" in status.nodes[0], false);
  assert.equal(JSON.stringify(status).includes("b".repeat(64)), false);
});

test("replayed nonce is rejected with HTTP 409", async () => {
  const db = new MockDatabase();
  const env = { RIGOS_STATUS_DB: db, RIGOS_STATUS_SECRET: SECRET };
  await acceptObservation(await signedRequest(observation()), env, NOW);
  const replayRequest = await signedRequest(observation());
  await assert.rejects(
    () => acceptObservation(replayRequest, env, NOW),
    (error) => error.code === "replay_detected" && error.status === 409,
  );
});

test("wrong secret is rejected before database mutation", async () => {
  const db = new MockDatabase();
  const env = { RIGOS_STATUS_DB: db, RIGOS_STATUS_SECRET: SECRET };
  const invalidRequest = await signedRequest(
    observation(),
    { secret: "f".repeat(64) },
  );
  await assert.rejects(
    () => acceptObservation(invalidRequest, env, NOW),
    (error) => error.code === "signature_mismatch" && error.status === 401,
  );
  assert.equal(db.rows.size, 0);
});

test("freshness becomes stale and offline without changing stored evidence", async () => {
  const db = new MockDatabase();
  const env = { RIGOS_STATUS_DB: db, RIGOS_STATUS_SECRET: SECRET };
  await acceptObservation(await signedRequest(observation()), env, NOW);

  assert.equal((await readPublicStatus(env, NOW + 91)).nodes[0].connection, "stale");
  assert.equal((await readPublicStatus(env, NOW + 301)).nodes[0].connection, "offline");
});
