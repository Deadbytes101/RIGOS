import {
  MAX_BODY_BYTES,
  OBSERVATION_SCHEMA,
  StatusError,
  jsonResponse,
  sourceKeyRegistry,
  validateAndSanitizeObservation,
  verifySignature,
  worstComponentStatus,
} from "./status-v2.js";

const NONCE_TTL_SECONDS = 600;
const RANDOMX_MSR_UNIT = "rigos-randomx-msr.service";

const ACTIVE_STATES = new Set([
  "active",
  "inactive",
  "failed",
  "activating",
  "deactivating",
  "unknown",
]);
const SUB_STATES = new Set([
  "running",
  "exited",
  "dead",
  "failed",
  "waiting",
  "unknown",
]);
const RESULTS = new Set([
  "success",
  "exit-code",
  "signal",
  "timeout",
  "unknown",
]);
const YES_NO_UNKNOWN = new Set(["yes", "no", "unknown"]);
const UNIT_FILE_STATES = new Set([
  "enabled",
  "disabled",
  "static",
  "masked",
  "indirect",
  "generated",
  "unknown",
]);

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function requireObject(value, label) {
  if (!isPlainObject(value)) {
    throw new StatusError(422, "invalid_observation", `${label} must be an object.`);
  }
  return value;
}

function requireExactKeys(value, allowed, label) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) {
      throw new StatusError(422, "invalid_observation", `${label}.${key} is not supported.`);
    }
  }
}

function requireString(value, label, maximum = 240) {
  if (typeof value !== "string" || value.length < 1 || value.length > maximum) {
    throw new StatusError(
      422,
      "invalid_observation",
      `${label} must be a non-empty string no longer than ${maximum} characters.`,
    );
  }
  return value;
}

function requireEnum(value, values, label) {
  if (typeof value !== "string" || !values.has(value)) {
    throw new StatusError(422, "invalid_observation", `${label} has an unsupported value.`);
  }
  return value;
}

function outcomeForStatus(status) {
  switch (status) {
    case "operational":
      return "ready";
    case "degraded":
      return "degraded";
    case "partial_outage":
    case "major_outage":
      return "failed";
    default:
      return "unknown";
  }
}

function legacyRandomxComponent(raw) {
  if (!isPlainObject(raw) || !Array.isArray(raw.components)) return null;
  const matches = raw.components.filter(
    (component) => isPlainObject(component) && component.id === "randomx-msr",
  );
  if (matches.length !== 1) return null;
  const component = matches[0];
  if (!isPlainObject(component.evidence)) return null;
  return component.evidence.unit === RANDOMX_MSR_UNIT ? component : null;
}

function validateLegacyRandomxObservation(raw, requestTimestamp) {
  const sourceComponent = legacyRandomxComponent(raw);
  if (!sourceComponent) {
    return validateAndSanitizeObservation(raw, requestTimestamp);
  }

  const label = "observation.components[randomx-msr].evidence";
  const evidence = requireObject(sourceComponent.evidence, label);
  requireExactKeys(
    evidence,
    new Set([
      "authority",
      "summary",
      "unit",
      "activeState",
      "subState",
      "result",
      "facts",
    ]),
    label,
  );

  const authority = requireString(evidence.authority, `${label}.authority`, 80);
  if (authority !== "rigos-status-agent") {
    throw new StatusError(422, "invalid_observation", `${label}.authority is not trusted.`);
  }
  requireString(evidence.summary, `${label}.summary`, 240);
  if (evidence.unit !== RANDOMX_MSR_UNIT) {
    throw new StatusError(422, "invalid_observation", `${label}.unit is not allowed for randomx-msr.`);
  }

  const activeState = requireEnum(evidence.activeState, ACTIVE_STATES, `${label}.activeState`);
  const subState = requireEnum(evidence.subState, SUB_STATES, `${label}.subState`);
  const result = requireEnum(evidence.result, RESULTS, `${label}.result`);

  const facts = requireObject(evidence.facts, `${label}.facts`);
  requireExactKeys(
    facts,
    new Set(["conditionResult", "unitFileState"]),
    `${label}.facts`,
  );
  const conditionResult = requireEnum(
    facts.conditionResult,
    YES_NO_UNKNOWN,
    `${label}.facts.conditionResult`,
  );
  const unitFileState = requireEnum(
    facts.unitFileState,
    UNIT_FILE_STATES,
    `${label}.facts.unitFileState`,
  );

  const normalized = JSON.parse(JSON.stringify(raw));
  const normalizedComponent = normalized.components.find(
    (component) => component?.id === "randomx-msr",
  );
  const outcome = outcomeForStatus(sourceComponent.status);
  normalizedComponent.evidence = {
    authority: "rigos-status-agent",
    summary: `RandomX MSR: ${outcome}`,
    schema: "rigos.randomx-msr-status/v1",
    outcome,
    facts: { verificationOutcome: outcome },
  };

  const sanitized = validateAndSanitizeObservation(normalized, requestTimestamp);
  const sanitizedComponent = sanitized.observation.components.find(
    (component) => component.id === "randomx-msr",
  );
  sanitizedComponent.evidence = {
    authority: "rigos-status-agent",
    unit: RANDOMX_MSR_UNIT,
    activeState,
    subState,
    result,
    facts: {
      conditionResult,
      unitFileState,
    },
    summary: `RandomX MSR: ${activeState}/${subState}`,
  };
  return sanitized;
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

  const contentType = String(request.headers.get("content-type") || "").toLowerCase();
  const mediaType = contentType.split(";", 1)[0].trim();
  if (mediaType !== "application/json") {
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

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    throw new StatusError(400, "invalid_json", "Observation body is not valid JSON.");
  }
}

function extractSourceId(raw) {
  const observation = requireObject(raw, "observation");
  const sourceId = requireString(observation.sourceId, "observation.sourceId", 64);
  if (!/^[a-f0-9]{64}$/.test(sourceId)) {
    throw new StatusError(
      422,
      "invalid_observation",
      "observation.sourceId must be 64 lowercase hexadecimal characters.",
    );
  }
  return sourceId;
}

function requireDatabase(env) {
  if (!env?.RIGOS_STATUS_DB) {
    throw new StatusError(503, "database_unavailable", "RIGOS_STATUS_DB is not bound.");
  }
  return env.RIGOS_STATUS_DB;
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
      throw new StatusError(
        503,
        "database_not_migrated",
        "Apply migrations/0001_status.sql to RIGOS_STATUS_DB.",
      );
    }
    throw error;
  }
  return { receivedAt };
}

async function acceptObservation(request, env, nowUnix = Math.floor(Date.now() / 1000)) {
  const db = requireDatabase(env);
  const body = await readBody(request);
  const parsed = parseJson(body.text);
  const sourceId = extractSourceId(parsed);
  const secret = sourceKeyRegistry(env).get(sourceId);
  if (!secret) {
    throw new StatusError(401, "unknown_source", "The signed source ID is not registered.");
  }

  const signed = await verifySignature(request, secret, body.bytes, nowUnix);
  const sanitized = validateLegacyRandomxObservation(parsed, signed.timestamp);
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

export { acceptObservation, validateLegacyRandomxObservation };
