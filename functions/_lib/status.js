const OBSERVATION_SCHEMA = "rigos.status-observation/v1";
const PUBLIC_SCHEMA = "rigos.public-status/v1";
const MAX_BODY_BYTES = 65536;
const MAX_CLOCK_SKEW_SECONDS = 300;
const NONCE_TTL_SECONDS = 600;
const LIVE_AFTER_SECONDS = 90;
const OFFLINE_AFTER_SECONDS = 300;

const COMPONENT_IDS = Object.freeze([
  "boot-device-verification",
  "persistent-state",
  "state-readiness",
  "recovery-access",
  "ssh-host-identity",
  "configuration-activation",
  "profile-apply",
  "runtime-render",
  "huge-page-authority",
  "randomx-msr",
  "network-readiness",
  "root-filesystem-integrity",
  "failed-unit-set",
  "operator-health",
  "kernel-integrity",
  "state-capacity",
  "time-synchronization",
  "ssh-service",
  "remote-access-observer",
]);

const COMPONENT_ID_SET = new Set(COMPONENT_IDS);
const COMPONENT_STATUS = new Set([
  "operational",
  "unknown",
  "degraded",
  "partial_outage",
  "major_outage",
]);
const HEALTH_STATUS = new Set(["ok", "degraded", "failed", "unavailable"]);
const SEVERITY = Object.freeze({
  operational: 0,
  unknown: 1,
  degraded: 2,
  partial_outage: 3,
  major_outage: 4,
});
const FORBIDDEN_KEYS = new Set([
  "wallet",
  "worker",
  "pool",
  "hashrate",
  "shares",
  "acceptedshares",
  "rejectedshares",
  "hostname",
  "ip",
  "ipaddress",
  "password",
  "secret",
  "apitoken",
  "privatekey",
]);

class StatusError extends Error {
  constructor(status, code, detail) {
    super(detail);
    this.name = "StatusError";
    this.status = status;
    this.code = code;
  }
}

function jsonResponse(value, status = 200, extraHeaders = {}) {
  return new Response(JSON.stringify(value), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
      ...extraHeaders,
    },
  });
}

function errorResponse(error) {
  if (error instanceof StatusError) {
    return jsonResponse(
      {
        accepted: false,
        error: error.code,
        detail: error.message,
      },
      error.status,
    );
  }

  console.error("rigos-status-service:", error);
  return jsonResponse(
    {
      accepted: false,
      error: "internal_error",
      detail: "The RIGOS status service could not complete the request.",
    },
    500,
  );
}

function methodNotAllowed(allowed) {
  return jsonResponse(
    {
      accepted: false,
      error: "method_not_allowed",
      detail: `Use ${allowed.join(" or ")}.`,
    },
    405,
    { allow: allowed.join(", ") },
  );
}

function requireRuntime(env) {
  if (!env?.RIGOS_STATUS_DB) {
    throw new StatusError(503, "database_unavailable", "RIGOS_STATUS_DB is not bound.");
  }

  const secret = String(env.RIGOS_STATUS_SECRET || "").trim().toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(secret)) {
    throw new StatusError(503, "secret_unavailable", "RIGOS_STATUS_SECRET is not configured.");
  }

  return { db: env.RIGOS_STATUS_DB, secret };
}

function requireDatabase(env) {
  if (!env?.RIGOS_STATUS_DB) {
    throw new StatusError(503, "database_unavailable", "RIGOS_STATUS_DB is not bound.");
  }
  return env.RIGOS_STATUS_DB;
}

async function readBody(request) {
  const rawLength = request.headers.get("content-length");
  if (rawLength !== null) {
    const length = Number(rawLength);
    if (!Number.isInteger(length) || length < 1) {
      throw new StatusError(400, "invalid_content_length", "Content-Length must be a positive integer.");
    }
    if (length > MAX_BODY_BYTES) {
      throw new StatusError(413, "payload_too_large", `Payload exceeds ${MAX_BODY_BYTES} bytes.`);
    }
  }

  const contentType = (request.headers.get("content-type") || "").toLowerCase();
  if (!contentType.startsWith("application/json")) {
    throw new StatusError(415, "unsupported_media_type", "Content-Type must be application/json.");
  }

  const bytes = new Uint8Array(await request.arrayBuffer());
  if (bytes.byteLength < 1) {
    throw new StatusError(400, "empty_payload", "Observation body is empty.");
  }
  if (bytes.byteLength > MAX_BODY_BYTES) {
    throw new StatusError(413, "payload_too_large", `Payload exceeds ${MAX_BODY_BYTES} bytes.`);
  }

  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new StatusError(400, "invalid_utf8", "Observation body is not valid UTF-8.");
  }

  return { bytes, text };
}

function hexToBytes(value) {
  const bytes = new Uint8Array(value.length / 2);
  for (let index = 0; index < value.length; index += 2) {
    bytes[index / 2] = Number.parseInt(value.slice(index, index + 2), 16);
  }
  return bytes;
}

function concatBytes(...parts) {
  const length = parts.reduce((total, part) => total + part.byteLength, 0);
  const output = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.byteLength;
  }
  return output;
}

async function verifySignature(request, secret, bodyBytes, nowUnix) {
  const timestampText = request.headers.get("x-rigos-timestamp") || "";
  const nonce = request.headers.get("x-rigos-nonce") || "";
  const signature = request.headers.get("x-rigos-signature") || "";

  if (!/^[0-9]{10,12}$/.test(timestampText)) {
    throw new StatusError(401, "invalid_timestamp", "X-RigOS-Timestamp is invalid.");
  }
  if (!/^[a-f0-9]{32}$/.test(nonce)) {
    throw new StatusError(401, "invalid_nonce", "X-RigOS-Nonce is invalid.");
  }
  if (!/^sha256=[a-f0-9]{64}$/.test(signature)) {
    throw new StatusError(401, "invalid_signature", "X-RigOS-Signature is invalid.");
  }

  const timestamp = Number(timestampText);
  if (!Number.isSafeInteger(timestamp)) {
    throw new StatusError(401, "invalid_timestamp", "X-RigOS-Timestamp is outside the supported range.");
  }

  const skew = Math.abs(nowUnix - timestamp);
  if (skew > MAX_CLOCK_SKEW_SECONDS) {
    throw new StatusError(401, "clock_skew", `Request clock differs by ${skew} seconds.`);
  }

  const encoder = new TextEncoder();
  const canonical = concatBytes(
    encoder.encode(`${timestampText}.${nonce}.`),
    bodyBytes,
  );
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["verify"],
  );
  const valid = await crypto.subtle.verify(
    "HMAC",
    key,
    hexToBytes(signature.slice(7)),
    canonical,
  );

  if (!valid) {
    throw new StatusError(401, "signature_mismatch", "Observation signature does not match the request body.");
  }

  return { timestamp, nonce };
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function requireObject(value, label) {
  if (!isPlainObject(value)) {
    throw new StatusError(422, "invalid_observation", `${label} must be an object.`);
  }
  return value;
}

function requireString(value, label, maximum = 240) {
  if (typeof value !== "string" || value.length < 1 || value.length > maximum) {
    throw new StatusError(422, "invalid_observation", `${label} must be a non-empty string no longer than ${maximum} characters.`);
  }
  return value;
}

function requireOptionalString(value, label, maximum = 240) {
  if (value === null || value === undefined) return null;
  return requireString(value, label, maximum);
}

function requireIsoTimestamp(value, label) {
  requireString(value, label, 64);
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed) || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/.test(value)) {
    throw new StatusError(422, "invalid_observation", `${label} must be an ISO-8601 UTC timestamp.`);
  }
  return Math.floor(parsed / 1000);
}

function normalizedKey(key) {
  return String(key).toLowerCase().replace(/[-_]/g, "");
}

function rejectForbiddenKeys(value, path = "observation") {
  if (Array.isArray(value)) {
    value.forEach((item, index) => rejectForbiddenKeys(item, `${path}[${index}]`));
    return;
  }
  if (!isPlainObject(value)) return;

  for (const [key, child] of Object.entries(value)) {
    if (FORBIDDEN_KEYS.has(normalizedKey(key))) {
      throw new StatusError(422, "private_field_rejected", `${path}.${key} is not allowed in public RIGOS status evidence.`);
    }
    rejectForbiddenKeys(child, `${path}.${key}`);
  }
}

function copyScalarFacts(value, label) {
  if (value === null || value === undefined) return {};
  const facts = requireObject(value, label);
  const entries = Object.entries(facts);
  if (entries.length > 24) {
    throw new StatusError(422, "invalid_observation", `${label} contains too many fields.`);
  }

  const output = {};
  for (const [key, item] of entries) {
    if (!/^[A-Za-z][A-Za-z0-9]{0,63}$/.test(key)) {
      throw new StatusError(422, "invalid_observation", `${label}.${key} has an invalid key.`);
    }
    if (FORBIDDEN_KEYS.has(normalizedKey(key))) {
      throw new StatusError(422, "private_field_rejected", `${label}.${key} is forbidden.`);
    }
    if (
      item !== null &&
      typeof item !== "string" &&
      typeof item !== "number" &&
      typeof item !== "boolean"
    ) {
      throw new StatusError(422, "invalid_observation", `${label}.${key} must be a scalar value.`);
    }
    if (typeof item === "string" && item.length > 160) {
      throw new StatusError(422, "invalid_observation", `${label}.${key} is too long.`);
    }
    if (typeof item === "number" && !Number.isFinite(item)) {
      throw new StatusError(422, "invalid_observation", `${label}.${key} is not finite.`);
    }
    output[key] = item;
  }
  return output;
}

function sanitizeEvidence(value, label) {
  const evidence = requireObject(value, label);
  const authority = requireString(evidence.authority, `${label}.authority`, 80);
  if (authority !== "rigos-status-agent") {
    throw new StatusError(422, "invalid_observation", `${label}.authority is not trusted.`);
  }

  const output = {
    authority,
    summary: requireString(evidence.summary, `${label}.summary`, 240),
  };

  for (const [source, target, maximum] of [
    ["unit", "unit", 160],
    ["activeState", "activeState", 80],
    ["subState", "subState", 80],
    ["result", "result", 80],
    ["schema", "schema", 160],
    ["outcome", "outcome", 128],
  ]) {
    const value = requireOptionalString(evidence[source], `${label}.${source}`, maximum);
    if (value !== null) output[target] = value;
  }

  const facts = copyScalarFacts(evidence.facts, `${label}.facts`);
  if (Object.keys(facts).length > 0) output.facts = facts;
  return output;
}

function validateAndSanitizeObservation(raw, requestTimestamp) {
  const observation = requireObject(raw, "observation");
  rejectForbiddenKeys(observation);

  if (observation.schema !== OBSERVATION_SCHEMA) {
    throw new StatusError(422, "unsupported_schema", `Expected ${OBSERVATION_SCHEMA}.`);
  }

  const observedUnix = requireIsoTimestamp(observation.observedAt, "observation.observedAt");
  if (Math.abs(observedUnix - requestTimestamp) > MAX_CLOCK_SKEW_SECONDS) {
    throw new StatusError(422, "observation_clock_skew", "Observation time does not match the signed request timestamp.");
  }

  const sourceId = requireString(observation.sourceId, "observation.sourceId", 64);
  if (!/^[a-f0-9]{64}$/.test(sourceId)) {
    throw new StatusError(422, "invalid_observation", "observation.sourceId must be 64 lowercase hexadecimal characters.");
  }

  const bootIdHash = requireString(observation.bootIdHash, "observation.bootIdHash", 64);
  if (!/^[a-f0-9]{64}$/.test(bootIdHash)) {
    throw new StatusError(422, "invalid_observation", "observation.bootIdHash must be 64 lowercase hexadecimal characters.");
  }

  const release = requireObject(observation.release, "observation.release");
  const sanitizedRelease = {
    product: requireString(release.product, "observation.release.product", 32),
    version: requireString(release.version, "observation.release.version", 128),
    imageId: requireOptionalString(release.imageId, "observation.release.imageId", 128),
    imageVersion: requireOptionalString(release.imageVersion, "observation.release.imageVersion", 128),
    channel: requireOptionalString(release.channel, "observation.release.channel", 64),
    buildId: requireOptionalString(release.buildId, "observation.release.buildId", 128),
    buildCommit: requireOptionalString(release.buildCommit, "observation.release.buildCommit", 64),
    architecture: requireOptionalString(release.architecture, "observation.release.architecture", 64),
  };
  if (sanitizedRelease.product !== "RIGOS") {
    throw new StatusError(422, "invalid_product", "Only RIGOS observations are accepted.");
  }
  if (sanitizedRelease.imageId !== null && sanitizedRelease.imageId !== "rigos-usb-amd64") {
    throw new StatusError(422, "invalid_product", "Observation imageId is not a supported RIGOS appliance image.");
  }
  if (sanitizedRelease.buildCommit !== null && !/^[a-f0-9]{40}$/.test(sanitizedRelease.buildCommit)) {
    throw new StatusError(422, "invalid_observation", "release.buildCommit must be a 40-character lowercase commit SHA.");
  }

  const health = requireObject(observation.health, "observation.health");
  if (!HEALTH_STATUS.has(health.status)) {
    throw new StatusError(422, "invalid_observation", "observation.health.status is invalid.");
  }
  if (health.exitCode !== null && (!Number.isInteger(health.exitCode) || health.exitCode < 0 || health.exitCode > 255)) {
    throw new StatusError(422, "invalid_observation", "observation.health.exitCode must be null or an integer from 0 to 255.");
  }
  const sanitizedHealth = {
    status: health.status,
    exitCode: health.exitCode,
    summary: requireString(health.summary, "observation.health.summary", 240),
  };

  if (!Array.isArray(observation.components) || observation.components.length !== COMPONENT_IDS.length) {
    throw new StatusError(422, "component_registry_mismatch", `Observation must contain exactly ${COMPONENT_IDS.length} components.`);
  }

  const seen = new Set();
  const components = observation.components.map((item, index) => {
    const component = requireObject(item, `observation.components[${index}]`);
    const id = requireString(component.id, `observation.components[${index}].id`, 80);
    if (!COMPONENT_ID_SET.has(id) || seen.has(id)) {
      throw new StatusError(422, "component_registry_mismatch", `Component ${id} is unknown or duplicated.`);
    }
    seen.add(id);

    if (!COMPONENT_STATUS.has(component.status)) {
      throw new StatusError(422, "invalid_observation", `Component ${id} has an invalid status.`);
    }
    if (component.observedAt !== observation.observedAt) {
      throw new StatusError(422, "invalid_observation", `Component ${id} has a mismatched observedAt value.`);
    }

    return {
      id,
      status: component.status,
      observedAt: observation.observedAt,
      evidence: sanitizeEvidence(component.evidence, `observation.components[${index}].evidence`),
    };
  });

  if (seen.size !== COMPONENT_IDS.length) {
    throw new StatusError(422, "component_registry_mismatch", "Observation component registry is incomplete.");
  }

  return {
    observedUnix,
    observation: {
      schema: OBSERVATION_SCHEMA,
      observedAt: observation.observedAt,
      sourceId,
      bootIdHash,
      release: sanitizedRelease,
      health: sanitizedHealth,
      components,
    },
  };
}

function worstComponentStatus(components) {
  let status = "operational";
  for (const component of components) {
    if (SEVERITY[component.status] > SEVERITY[status]) status = component.status;
  }
  return status;
}

async function storeObservation(db, signed, sanitized, nowUnix) {
  const receivedAt = new Date(nowUnix * 1000).toISOString().replace(".000Z", "Z");
  const payloadJson = JSON.stringify(sanitized.observation);
  const sourceId = sanitized.observation.sourceId;
  const overallStatus = worstComponentStatus(sanitized.observation.components);
  const nonceExpires = signed.timestamp + NONCE_TTL_SECONDS;

  const statements = [
    db.prepare("DELETE FROM ingest_nonces WHERE expires_at < ?").bind(nowUnix),
    db.prepare(
      "INSERT INTO ingest_nonces (nonce, request_timestamp, source_id, expires_at) VALUES (?, ?, ?, ?)",
    ).bind(signed.nonce, signed.timestamp, sourceId, nonceExpires),
    db.prepare(
      `INSERT INTO status_observations (
        source_id, observed_at, observed_unix, received_at, received_unix,
        release_version, build_commit, overall_status, payload_json
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(source_id) DO UPDATE SET
        observed_at = excluded.observed_at,
        observed_unix = excluded.observed_unix,
        received_at = excluded.received_at,
        received_unix = excluded.received_unix,
        release_version = excluded.release_version,
        build_commit = excluded.build_commit,
        overall_status = excluded.overall_status,
        payload_json = excluded.payload_json
      WHERE excluded.observed_unix >= status_observations.observed_unix`,
    ).bind(
      sourceId,
      sanitized.observation.observedAt,
      sanitized.observedUnix,
      receivedAt,
      nowUnix,
      sanitized.observation.release.version,
      sanitized.observation.release.buildCommit,
      overallStatus,
      payloadJson,
    ),
  ];

  try {
    await db.batch(statements);
  } catch (error) {
    const message = String(error?.message || error);
    if (message.includes("UNIQUE constraint failed") && message.includes("ingest_nonces")) {
      throw new StatusError(409, "replay_detected", "This signed nonce has already been accepted.");
    }
    if (message.includes("no such table")) {
      throw new StatusError(503, "database_not_migrated", "Apply migrations/0001_status.sql to RIGOS_STATUS_DB.");
    }
    throw error;
  }

  return { receivedAt, overallStatus };
}

async function acceptObservation(request, env, nowUnix = Math.floor(Date.now() / 1000)) {
  const { db, secret } = requireRuntime(env);
  const body = await readBody(request);
  const signed = await verifySignature(request, secret, body.bytes, nowUnix);

  let parsed;
  try {
    parsed = JSON.parse(body.text);
  } catch {
    throw new StatusError(400, "invalid_json", "Observation body is not valid JSON.");
  }

  const sanitized = validateAndSanitizeObservation(parsed, signed.timestamp);
  const stored = await storeObservation(db, signed, sanitized, nowUnix);

  return jsonResponse(
    {
      accepted: true,
      schema: OBSERVATION_SCHEMA,
      source: sanitized.observation.sourceId.slice(0, 12),
      observedAt: sanitized.observation.observedAt,
      receivedAt: stored.receivedAt,
    },
    202,
  );
}

function parseStoredObservation(row) {
  try {
    const observation = JSON.parse(row.payload_json);
    if (!isPlainObject(observation) || observation.schema !== OBSERVATION_SCHEMA) return null;
    return observation;
  } catch {
    return null;
  }
}

function connectionState(receivedUnix, nowUnix) {
  const ageSeconds = Math.max(0, nowUnix - receivedUnix);
  if (ageSeconds <= LIVE_AFTER_SECONDS) return { state: "live", ageSeconds };
  if (ageSeconds <= OFFLINE_AFTER_SECONDS) return { state: "stale", ageSeconds };
  return { state: "offline", ageSeconds };
}

function publicNode(row, nowUnix) {
  const observation = parseStoredObservation(row);
  if (!observation) return null;

  const connection = connectionState(Number(row.received_unix), nowUnix);
  const componentStatus = worstComponentStatus(observation.components);
  const systemState = connection.state === "live" ? componentStatus : connection.state;

  return {
    nodeId: String(row.source_id).slice(0, 12),
    connection: connection.state,
    ageSeconds: connection.ageSeconds,
    systemState,
    componentState: componentStatus,
    observedAt: observation.observedAt,
    receivedAt: row.received_at,
    release: observation.release,
    health: observation.health,
    components: observation.components,
  };
}

async function readPublicStatus(env, nowUnix = Math.floor(Date.now() / 1000)) {
  const db = requireDatabase(env);
  let result;
  try {
    result = await db.prepare(
      `SELECT source_id, received_at, received_unix, payload_json
       FROM status_observations
       ORDER BY received_unix DESC
       LIMIT 32`,
    ).all();
  } catch (error) {
    const message = String(error?.message || error);
    if (message.includes("no such table")) {
      throw new StatusError(503, "database_not_migrated", "Apply migrations/0001_status.sql to RIGOS_STATUS_DB.");
    }
    throw error;
  }

  const nodes = (result?.results || [])
    .map((row) => publicNode(row, nowUnix))
    .filter(Boolean);

  return {
    schema: PUBLIC_SCHEMA,
    generatedAt: new Date(nowUnix * 1000).toISOString().replace(".000Z", "Z"),
    nodeCount: nodes.length,
    nodes,
  };
}

function publicStatusResponse(value) {
  return jsonResponse(value, 200, {
    "access-control-allow-origin": "*",
    "access-control-allow-methods": "GET, HEAD",
  });
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function humanAge(seconds) {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

function statusLabel(value) {
  return String(value).replaceAll("_", " ").toUpperCase();
}

function statusClass(value) {
  return `state-${String(value).replaceAll("_", "-")}`;
}

function componentRows(components) {
  return components.map((component) => `
    <tr>
      <th scope="row">${escapeHtml(component.id)}</th>
      <td><span class="state ${statusClass(component.status)}">${escapeHtml(statusLabel(component.status))}</span></td>
      <td>${escapeHtml(component.evidence.summary)}</td>
    </tr>`).join("");
}

function nodeSection(node) {
  const release = node.release || {};
  const build = release.buildCommit ? release.buildCommit.slice(0, 12) : "unavailable";
  return `
  <section class="status-node" aria-labelledby="node-${escapeHtml(node.nodeId)}">
    <div class="status-node-heading">
      <h2 id="node-${escapeHtml(node.nodeId)}">RIGOS NODE ${escapeHtml(node.nodeId)}</h2>
      <span class="state ${statusClass(node.systemState)}">${escapeHtml(statusLabel(node.systemState))}</span>
    </div>
    <dl class="status">
      <dt>Connection</dt><dd><span class="state ${statusClass(node.connection)}">${escapeHtml(statusLabel(node.connection))}</span></dd>
      <dt>Last received</dt><dd>${escapeHtml(node.receivedAt)} (${escapeHtml(humanAge(node.ageSeconds))} ago)</dd>
      <dt>Observed</dt><dd>${escapeHtml(node.observedAt)}</dd>
      <dt>Release</dt><dd>${escapeHtml(release.version || "unknown")}</dd>
      <dt>Build</dt><dd>${escapeHtml(build)}</dd>
      <dt>Architecture</dt><dd>${escapeHtml(release.architecture || "unknown")}</dd>
      <dt>Rig health</dt><dd>${escapeHtml(node.health.status)} — ${escapeHtml(node.health.summary)}</dd>
      <dt>Components</dt><dd>${node.components.length}</dd>
    </dl>
    <table class="component-table">
      <thead><tr><th>Component</th><th>State</th><th>Evidence</th></tr></thead>
      <tbody>${componentRows(node.components)}</tbody>
    </table>
  </section>`;
}

function summaryCounts(nodes) {
  const counts = { live: 0, stale: 0, offline: 0 };
  for (const node of nodes) counts[node.connection] += 1;
  return counts;
}

function renderStatusPage(publicStatus, statusCode = 200, notice = null) {
  const nodes = publicStatus?.nodes || [];
  const counts = summaryCounts(nodes);
  const body = notice
    ? `<div class="status-empty"><h2>Status service unavailable</h2><p>${escapeHtml(notice)}</p></div>`
    : nodes.length === 0
      ? `<div class="status-empty"><h2>No RIGOS observations yet</h2><p>The endpoint is ready, but no signed appliance observation has been accepted.</p></div>`
      : nodes.map(nodeSection).join("");

  const generatedAt = publicStatus?.generatedAt || new Date().toISOString();
  const html = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="30">
  <title>RIGOS System Status</title>
  <meta name="description" content="Direct signed system status from RIGOS appliances. No mining account, wallet, pool, worker, hashrate or remote-control surface.">
  <meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1">
  <meta name="theme-color" content="#111214">
  <link rel="canonical" href="https://rigos.site/status">
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/style.css">
  <link rel="stylesheet" href="/status.css">
</head>
<body>
<a class="skip-link" href="#content">Skip to content</a>
<header class="site-header">
  <div class="shell">
    <a class="brand" href="/">RIGOS</a>
    <p class="subtitle">Direct signed appliance observation. Read-only. No cloud owner.</p>
    <nav class="nav" aria-label="Site navigation">
      <a href="/">Overview</a>
      <a href="/status" aria-current="page">System status</a>
      <a href="/history.html">History</a>
      <a href="/architecture.html">Architecture</a>
      <a href="/evidence.html">Evidence</a>
      <a href="/limits.html">Limits</a>
      <a href="https://github.com/Deadbytes101/RIGOS">Source code</a>
    </nav>
  </div>
</header>
<main id="content" class="shell status-shell">
  <h1>RIGOS system status</h1>
  <p class="lead">This page reports signed operating-system evidence emitted by RIGOS itself. It is not a mining account, fleet controller or HiveOS clone.</p>
  <div class="callout"><strong>Privacy boundary:</strong> no wallet, pool, worker name, hashrate, shares, hostname, IP address, password, token or remote command is accepted or published.</div>
  <dl class="status status-summary">
    <dt>Observed nodes</dt><dd>${nodes.length}</dd>
    <dt>Live</dt><dd>${counts.live}</dd>
    <dt>Stale</dt><dd>${counts.stale}</dd>
    <dt>Offline</dt><dd>${counts.offline}</dd>
    <dt>Generated</dt><dd>${escapeHtml(generatedAt)}</dd>
    <dt>Refresh</dt><dd>30 seconds</dd>
  </dl>
  ${body}
  <p class="note">RIGOS remains an experimental Alpha appliance. Status evidence reports what the machine observed; it does not claim broad hardware compatibility or production readiness.</p>
</main>
<footer class="site-footer">
  <div class="shell">
    <p>RIGOS direct system status. Server-rendered Cloudflare Pages Function. Zero browser JavaScript.</p>
    <p><a href="/api/v1/status">Public JSON status</a> · <a href="https://github.com/Deadbytes101/RIGOS">Source repository</a></p>
  </div>
</footer>
</body>
</html>`;

  return new Response(html, {
    status: statusCode,
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
      "content-security-policy": "default-src 'none'; style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
      "referrer-policy": "no-referrer",
      "x-content-type-options": "nosniff",
      "x-frame-options": "DENY",
    },
  });
}

export {
  COMPONENT_IDS,
  MAX_BODY_BYTES,
  OBSERVATION_SCHEMA,
  PUBLIC_SCHEMA,
  StatusError,
  acceptObservation,
  errorResponse,
  jsonResponse,
  methodNotAllowed,
  publicStatusResponse,
  readPublicStatus,
  renderStatusPage,
  validateAndSanitizeObservation,
  verifySignature,
  worstComponentStatus,
};
