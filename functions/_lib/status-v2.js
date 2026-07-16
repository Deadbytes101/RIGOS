const OBSERVATION_SCHEMA = "rigos.status-observation/v1";
const PUBLIC_SCHEMA = "rigos.public-status/v1";
const MAX_BODY_BYTES = 65536;
const MAX_CLOCK_SKEW_SECONDS = 300;
const NONCE_TTL_SECONDS = 600;
const LIVE_AFTER_SECONDS = 90;
const OFFLINE_AFTER_SECONDS = 300;
const PUBLIC_NODE_LIMIT = 32;
const MAX_SOURCE_KEYS = 64;

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

const SENSITIVE_KEY_EXACT = new Set([
  "ip",
  "ipaddress",
  "acceptedshares",
  "rejectedshares",
]);
const SENSITIVE_KEY_TOKENS = Object.freeze([
  "wallet",
  "worker",
  "pool",
  "hashrate",
  "share",
  "hostname",
  "password",
  "secret",
  "token",
  "privatekey",
  "miningidentity",
  "credential",
  "account",
]);

const ACTIVE_STATES = new Set(["active", "inactive", "failed", "activating", "deactivating", "unknown"]);
const SUB_STATES = new Set(["running", "exited", "dead", "failed", "waiting", "unknown"]);
const UNIT_FILE_STATES = new Set(["enabled", "disabled", "static", "masked", "indirect", "generated", "unknown"]);
const RESULTS = new Set(["success", "exit-code", "signal", "timeout", "unknown"]);
const YES_NO_UNKNOWN = new Set(["yes", "no", "unknown"]);
const READY_STATES = new Set(["ready", "verified", "unavailable", "unknown", "failed", "blocked", "degraded"]);

const COMPONENT_POLICY = Object.freeze({
  "boot-device-verification": {
    label: "Boot-device verification",
    schemas: new Set(["rigos.boot-device/v1"]),
    outcomes: READY_STATES,
    facts: { verificationOutcome: enumRule(READY_STATES) },
  },
  "persistent-state": {
    label: "Persistent state",
    schemas: new Set(["rigos.state-status/v1"]),
    outcomes: READY_STATES,
    facts: { stateOutcome: enumRule(READY_STATES) },
  },
  "state-readiness": {
    label: "State readiness",
    unit: "rigos-state-ready.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
  "recovery-access": {
    label: "Recovery access",
    schemas: new Set(["rigos.recovery-access-status/v1"]),
    outcomes: READY_STATES,
    facts: { stateOutcome: enumRule(READY_STATES) },
  },
  "ssh-host-identity": {
    label: "SSH host identity",
    unit: "rigos-ssh-hostkeys.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
  "configuration-activation": {
    label: "Configuration activation",
    schemas: new Set(["rigos.activation-status/v1"]),
    outcomes: READY_STATES,
    facts: { configurationState: enumRule(new Set(["ready", "unconfigured", "unavailable", "unknown", "failed"])) },
  },
  "profile-apply": {
    label: "Profile apply",
    unit: "rigos-profile-apply.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
  "runtime-render": {
    label: "Runtime render",
    unit: "rigos-runtime-render.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
  "huge-page-authority": {
    label: "Huge-page authority",
    schemas: new Set(["rigos.performance-status/v1"]),
    outcomes: READY_STATES,
    facts: {
      actualPages: integerRule(0, 1_000_000_000),
      allocationPercent: numberRule(0, 100),
      targetPages: integerRule(0, 1_000_000_000),
    },
  },
  "randomx-msr": {
    label: "RandomX MSR",
    schemas: new Set(["rigos.randomx-msr-status/v1"]),
    outcomes: READY_STATES,
    facts: { verificationOutcome: enumRule(READY_STATES) },
  },
  "network-readiness": {
    label: "Network readiness",
    unit: "NetworkManager.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
  "root-filesystem-integrity": {
    label: "Root filesystem integrity",
    facts: {
      immutableLowerLayer: booleanRule(),
      rootFileSystem: enumRule(new Set(["overlay", "squashfs", "ext4", "unknown"])),
      rootMode: enumRule(new Set(["read-only", "read-write", "unknown"])),
      rootReadOnly: booleanRule(),
    },
  },
  "failed-unit-set": {
    label: "Failed unit set",
    facts: {
      failedUnits: integerRule(0, 100_000),
      ignoredObserverFailures: integerRule(0, 100_000),
    },
  },
  "operator-health": {
    label: "Operator health",
    facts: { commandExitCode: integerRule(0, 255) },
  },
  "kernel-integrity": {
    label: "Kernel integrity",
    facts: {
      severeEvents: integerRule(0, 1_000_000),
      warningEvents: integerRule(0, 1_000_000),
    },
  },
  "state-capacity": {
    label: "State capacity",
    facts: { freePercent: numberRule(0, 100) },
  },
  "time-synchronization": {
    label: "Time synchronization",
    facts: {
      ntpSynchronized: booleanRule(),
      timeServiceActiveState: enumRule(ACTIVE_STATES),
    },
  },
  "ssh-service": {
    label: "SSH service",
    unit: "ssh.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
  "remote-access-observer": {
    label: "Remote-access observer",
    unit: "rigos-remote-access-observe.service",
    facts: {
      conditionResult: enumRule(YES_NO_UNKNOWN),
      unitFileState: enumRule(UNIT_FILE_STATES),
    },
  },
});

class StatusError extends Error {
  constructor(status, code, detail) {
    super(detail);
    this.name = "StatusError";
    this.status = status;
    this.code = code;
  }
}

function enumRule(values) {
  return Object.freeze({ type: "enum", values });
}

function integerRule(minimum, maximum) {
  return Object.freeze({ type: "integer", minimum, maximum });
}

function numberRule(minimum, maximum) {
  return Object.freeze({ type: "number", minimum, maximum });
}

function booleanRule() {
  return Object.freeze({ type: "boolean" });
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
      { accepted: false, error: error.code, detail: error.message },
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

function requireDatabase(env) {
  if (!env?.RIGOS_STATUS_DB) {
    throw new StatusError(503, "database_unavailable", "RIGOS_STATUS_DB is not bound.");
  }
  return env.RIGOS_STATUS_DB;
}

function normalizeHex64(value, label) {
  const normalized = String(value || "").trim().toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(normalized)) {
    throw new StatusError(503, "source_registry_unavailable", `${label} must be 64 hexadecimal characters.`);
  }
  return normalized;
}

function sourceKeyRegistry(env) {
  const serialized = String(env?.RIGOS_STATUS_SOURCE_KEYS || "").trim();
  if (serialized) {
    let raw;
    try {
      raw = JSON.parse(serialized);
    } catch {
      throw new StatusError(503, "source_registry_unavailable", "RIGOS_STATUS_SOURCE_KEYS is not valid JSON.");
    }
    if (!isPlainObject(raw)) {
      throw new StatusError(503, "source_registry_unavailable", "RIGOS_STATUS_SOURCE_KEYS must be a JSON object.");
    }
    const entries = Object.entries(raw);
    if (entries.length < 1 || entries.length > MAX_SOURCE_KEYS) {
      throw new StatusError(503, "source_registry_unavailable", `RIGOS_STATUS_SOURCE_KEYS must contain 1 to ${MAX_SOURCE_KEYS} sources.`);
    }
    const registry = new Map();
    for (const [sourceId, secret] of entries) {
      registry.set(
        normalizeHex64(sourceId, "RIGOS_STATUS_SOURCE_KEYS source ID"),
        normalizeHex64(secret, "RIGOS_STATUS_SOURCE_KEYS secret"),
      );
    }
    return registry;
  }

  const sourceIdRaw = String(env?.RIGOS_STATUS_SOURCE_ID || "").trim();
  const secretRaw = String(env?.RIGOS_STATUS_SECRET || "").trim();
  if (!sourceIdRaw && !secretRaw) {
    throw new StatusError(
      503,
      "source_registry_unavailable",
      "Configure RIGOS_STATUS_SOURCE_KEYS or both RIGOS_STATUS_SOURCE_ID and RIGOS_STATUS_SECRET.",
    );
  }
  if (!sourceIdRaw || !secretRaw) {
    throw new StatusError(
      503,
      "source_registry_unavailable",
      "RIGOS_STATUS_SOURCE_ID and RIGOS_STATUS_SECRET must be configured together.",
    );
  }
  return new Map([[
    normalizeHex64(sourceIdRaw, "RIGOS_STATUS_SOURCE_ID"),
    normalizeHex64(secretRaw, "RIGOS_STATUS_SECRET"),
  ]]);
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
  const canonical = concatBytes(encoder.encode(`${timestampText}.${nonce}.`), bodyBytes);
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

function requireExactKeys(value, allowed, label) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) {
      throw new StatusError(422, "invalid_observation", `${label}.${key} is not supported.`);
    }
  }
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

function requireSafeToken(value, label, maximum = 128) {
  const text = requireString(value, label, maximum);
  if (!/^[A-Za-z0-9][A-Za-z0-9._/+:-]*$/.test(text)) {
    throw new StatusError(422, "invalid_observation", `${label} contains unsupported characters.`);
  }
  return text;
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

function isSensitiveKey(key) {
  const normalized = normalizedKey(key);
  if (SENSITIVE_KEY_EXACT.has(normalized)) return true;
  return SENSITIVE_KEY_TOKENS.some((token) => normalized.includes(token));
}

function rejectSensitiveKeys(value, path = "observation") {
  if (Array.isArray(value)) {
    value.forEach((item, index) => rejectSensitiveKeys(item, `${path}[${index}]`));
    return;
  }
  if (!isPlainObject(value)) return;
  for (const [key, child] of Object.entries(value)) {
    if (isSensitiveKey(key)) {
      throw new StatusError(422, "private_field_rejected", `${path}.${key} is not allowed in public RIGOS status evidence.`);
    }
    rejectSensitiveKeys(child, `${path}.${key}`);
  }
}

function validateRule(value, rule, label) {
  switch (rule.type) {
    case "enum":
      if (typeof value !== "string" || !rule.values.has(value)) {
        throw new StatusError(422, "invalid_observation", `${label} has an unsupported value.`);
      }
      return value;
    case "integer":
      if (!Number.isInteger(value) || value < rule.minimum || value > rule.maximum) {
        throw new StatusError(422, "invalid_observation", `${label} is outside the supported integer range.`);
      }
      return value;
    case "number":
      if (typeof value !== "number" || !Number.isFinite(value) || value < rule.minimum || value > rule.maximum) {
        throw new StatusError(422, "invalid_observation", `${label} is outside the supported numeric range.`);
      }
      return value;
    case "boolean":
      if (typeof value !== "boolean") {
        throw new StatusError(422, "invalid_observation", `${label} must be a boolean.`);
      }
      return value;
    default:
      throw new StatusError(500, "internal_error", `Unknown validation rule for ${label}.`);
  }
}

function sanitizeFacts(value, componentId, label) {
  if (value === null || value === undefined) return {};
  const facts = requireObject(value, label);
  const policy = COMPONENT_POLICY[componentId];
  const allowed = policy.facts || {};
  requireExactKeys(facts, new Set(Object.keys(allowed)), label);
  const output = {};
  for (const [key, item] of Object.entries(facts)) {
    output[key] = validateRule(item, allowed[key], `${label}.${key}`);
  }
  return output;
}

function sanitizedSummary(componentId, status, evidence) {
  const policy = COMPONENT_POLICY[componentId];
  let detail = status.replaceAll("_", " ");
  if (evidence.outcome) detail = evidence.outcome;
  else if (evidence.activeState) detail = evidence.subState
    ? `${evidence.activeState}/${evidence.subState}`
    : evidence.activeState;
  else {
    const firstFact = Object.values(evidence.facts || {})[0];
    if (typeof firstFact === "string") detail = firstFact;
    else if (typeof firstFact === "boolean") detail = firstFact ? "yes" : "no";
  }
  return `${policy.label}: ${detail}`;
}

function sanitizeEvidence(value, componentId, status, label) {
  const evidence = requireObject(value, label);
  requireExactKeys(
    evidence,
    new Set(["authority", "summary", "unit", "activeState", "subState", "result", "schema", "outcome", "facts"]),
    label,
  );
  const policy = COMPONENT_POLICY[componentId];
  const authority = requireString(evidence.authority, `${label}.authority`, 80);
  if (authority !== "rigos-status-agent") {
    throw new StatusError(422, "invalid_observation", `${label}.authority is not trusted.`);
  }
  requireString(evidence.summary, `${label}.summary`, 240);

  const output = { authority };
  const unit = requireOptionalString(evidence.unit, `${label}.unit`, 160);
  if (unit !== null) {
    if (!policy.unit || unit !== policy.unit) {
      throw new StatusError(422, "invalid_observation", `${label}.unit is not allowed for ${componentId}.`);
    }
    output.unit = unit;
  }

  for (const [field, values] of [
    ["activeState", ACTIVE_STATES],
    ["subState", SUB_STATES],
    ["result", RESULTS],
  ]) {
    const candidate = requireOptionalString(evidence[field], `${label}.${field}`, 80);
    if (candidate !== null) {
      if (!policy.unit || !values.has(candidate)) {
        throw new StatusError(422, "invalid_observation", `${label}.${field} is not allowed for ${componentId}.`);
      }
      output[field] = candidate;
    }
  }

  const schema = requireOptionalString(evidence.schema, `${label}.schema`, 160);
  if (schema !== null) {
    if (!policy.schemas || !policy.schemas.has(schema)) {
      throw new StatusError(422, "invalid_observation", `${label}.schema is not allowed for ${componentId}.`);
    }
    output.schema = schema;
  }

  const outcome = requireOptionalString(evidence.outcome, `${label}.outcome`, 128);
  if (outcome !== null) {
    if (!policy.outcomes || !policy.outcomes.has(outcome)) {
      throw new StatusError(422, "invalid_observation", `${label}.outcome is not allowed for ${componentId}.`);
    }
    output.outcome = outcome;
  }

  const facts = sanitizeFacts(evidence.facts, componentId, `${label}.facts`);
  if (Object.keys(facts).length > 0) output.facts = facts;
  output.summary = sanitizedSummary(componentId, status, output);
  return output;
}

function extractSourceId(raw) {
  const observation = requireObject(raw, "observation");
  const sourceId = requireString(observation.sourceId, "observation.sourceId", 64);
  if (!/^[a-f0-9]{64}$/.test(sourceId)) {
    throw new StatusError(422, "invalid_observation", "observation.sourceId must be 64 lowercase hexadecimal characters.");
  }
  return sourceId;
}

function validateAndSanitizeObservation(raw, requestTimestamp) {
  const observation = requireObject(raw, "observation");
  rejectSensitiveKeys(observation);
  requireExactKeys(
    observation,
    new Set(["schema", "observedAt", "sourceId", "bootIdHash", "release", "health", "components"]),
    "observation",
  );

  if (observation.schema !== OBSERVATION_SCHEMA) {
    throw new StatusError(422, "unsupported_schema", `Expected ${OBSERVATION_SCHEMA}.`);
  }
  const observedUnix = requireIsoTimestamp(observation.observedAt, "observation.observedAt");
  if (Math.abs(observedUnix - requestTimestamp) > MAX_CLOCK_SKEW_SECONDS) {
    throw new StatusError(422, "observation_clock_skew", "Observation time does not match the signed request timestamp.");
  }

  const sourceId = extractSourceId(observation);
  const bootIdHash = requireString(observation.bootIdHash, "observation.bootIdHash", 64);
  if (!/^[a-f0-9]{64}$/.test(bootIdHash)) {
    throw new StatusError(422, "invalid_observation", "observation.bootIdHash must be 64 lowercase hexadecimal characters.");
  }

  const release = requireObject(observation.release, "observation.release");
  requireExactKeys(
    release,
    new Set(["product", "version", "imageId", "imageVersion", "channel", "buildId", "buildCommit", "architecture"]),
    "observation.release",
  );
  if (release.product !== "RIGOS") {
    throw new StatusError(422, "invalid_product", "Only RIGOS observations are accepted.");
  }
  const sanitizedRelease = {
    product: "RIGOS",
    version: requireSafeToken(release.version, "observation.release.version", 128),
    imageId: requireOptionalString(release.imageId, "observation.release.imageId", 128),
    imageVersion: release.imageVersion == null ? null : requireSafeToken(release.imageVersion, "observation.release.imageVersion", 128),
    channel: release.channel == null ? null : requireSafeToken(release.channel, "observation.release.channel", 32),
    buildId: release.buildId == null ? null : requireSafeToken(release.buildId, "observation.release.buildId", 128),
    buildCommit: requireOptionalString(release.buildCommit, "observation.release.buildCommit", 40),
    architecture: requireOptionalString(release.architecture, "observation.release.architecture", 64),
  };
  if (sanitizedRelease.imageId !== null && sanitizedRelease.imageId !== "rigos-usb-amd64") {
    throw new StatusError(422, "invalid_product", "Observation imageId is not a supported RIGOS appliance image.");
  }
  if (sanitizedRelease.buildCommit !== null && !/^[a-f0-9]{40}$/.test(sanitizedRelease.buildCommit)) {
    throw new StatusError(422, "invalid_observation", "release.buildCommit must be a 40-character lowercase commit SHA.");
  }
  if (sanitizedRelease.architecture !== null && sanitizedRelease.architecture !== "x86_64") {
    throw new StatusError(422, "invalid_observation", "release.architecture must be x86_64.");
  }

  const health = requireObject(observation.health, "observation.health");
  requireExactKeys(health, new Set(["status", "exitCode", "summary"]), "observation.health");
  if (!HEALTH_STATUS.has(health.status)) {
    throw new StatusError(422, "invalid_observation", "observation.health.status is invalid.");
  }
  if (health.exitCode !== null && (!Number.isInteger(health.exitCode) || health.exitCode < 0 || health.exitCode > 255)) {
    throw new StatusError(422, "invalid_observation", "observation.health.exitCode must be null or an integer from 0 to 255.");
  }
  requireString(health.summary, "observation.health.summary", 240);
  const sanitizedHealth = {
    status: health.status,
    exitCode: health.exitCode,
    summary: health.status === "ok" && health.exitCode === 0
      ? "rig health completed successfully"
      : `rig health reported ${health.status}`,
  };

  if (!Array.isArray(observation.components) || observation.components.length !== COMPONENT_IDS.length) {
    throw new StatusError(422, "component_registry_mismatch", `Observation must contain exactly ${COMPONENT_IDS.length} components.`);
  }

  const seen = new Set();
  const components = observation.components.map((item, index) => {
    const component = requireObject(item, `observation.components[${index}]`);
    requireExactKeys(component, new Set(["id", "status", "observedAt", "evidence"]), `observation.components[${index}]`);
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
      evidence: sanitizeEvidence(
        component.evidence,
        id,
        component.status,
        `observation.components[${index}].evidence`,
      ),
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
  const db = requireDatabase(env);
  const body = await readBody(request);
  const parsed = parseJson(body.text);
  const sourceId = extractSourceId(parsed);
  const secret = sourceKeyRegistry(env).get(sourceId);
  if (!secret) {
    throw new StatusError(401, "unknown_source", "The signed source ID is not registered.");
  }
  const signed = await verifySignature(request, secret, body.bytes, nowUnix);
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
  const componentState = worstComponentStatus(observation.components);
  return {
    nodeId: String(row.source_id).slice(0, 12),
    connection: connection.state,
    ageSeconds: connection.ageSeconds,
    systemState: componentState,
    componentState,
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
  let countResult;
  try {
    result = await db.prepare(
      `SELECT source_id, received_at, received_unix, payload_json
       FROM status_observations
       ORDER BY received_unix DESC
       LIMIT ${PUBLIC_NODE_LIMIT}`,
    ).all();
    countResult = await db.prepare("SELECT COUNT(*) AS total FROM status_observations").all();
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
  const totalNodeCount = Number(countResult?.results?.[0]?.total ?? nodes.length);

  return {
    schema: PUBLIC_SCHEMA,
    generatedAt: new Date(nowUnix * 1000).toISOString().replace(".000Z", "Z"),
    nodeCount: nodes.length,
    totalNodeCount,
    truncated: totalNodeCount > nodes.length,
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
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function renderStatusPage(publicStatus, statusCode = 200, notice = null) {
  const nodes = Array.isArray(publicStatus?.nodes) ? publicStatus.nodes : [];
  const body = notice
    ? `<h1>Status service unavailable</h1><p>${escapeHtml(notice)}</p>`
    : `<h1>RIGOS system status</h1><p>${nodes.length} observed systems.</p>`;
  return new Response(`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>RIGOS System Status</title></head><body>${body}</body></html>`, {
    status: statusCode,
    headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" },
  });
}

export {
  COMPONENT_IDS,
  MAX_BODY_BYTES,
  OBSERVATION_SCHEMA,
  PUBLIC_NODE_LIMIT,
  PUBLIC_SCHEMA,
  StatusError,
  acceptObservation,
  connectionState,
  errorResponse,
  jsonResponse,
  methodNotAllowed,
  publicStatusResponse,
  readPublicStatus,
  renderStatusPage,
  sourceKeyRegistry,
  validateAndSanitizeObservation,
  verifySignature,
  worstComponentStatus,
};
