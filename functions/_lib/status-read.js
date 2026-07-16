import {
  OBSERVATION_SCHEMA,
  PUBLIC_NODE_LIMIT,
  PUBLIC_SCHEMA,
  StatusError,
  worstComponentStatus,
} from "./status-v2.js";
import { validateLegacyRandomxObservation } from "./status-multi.js";

function requireDatabase(env) {
  if (!env?.RIGOS_STATUS_DB) {
    throw new StatusError(503, "database_unavailable", "RIGOS_STATUS_DB is not bound.");
  }
  return env.RIGOS_STATUS_DB;
}

function parseStoredObservation(row) {
  try {
    const raw = JSON.parse(row.payload_json);
    if (!raw || raw.schema !== OBSERVATION_SCHEMA) return null;
    const observedUnix = Math.floor(Date.parse(raw.observedAt) / 1000);
    if (!Number.isFinite(observedUnix)) return null;
    const sanitized = validateLegacyRandomxObservation(raw, observedUnix).observation;
    if (sanitized.sourceId !== String(row.source_id)) return null;
    return sanitized;
  } catch {
    return null;
  }
}

function connectionState(receivedUnix, nowUnix) {
  const ageSeconds = Math.max(0, nowUnix - receivedUnix);
  if (ageSeconds <= 90) return { state: "live", ageSeconds };
  if (ageSeconds <= 300) return { state: "stale", ageSeconds };
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

export { readPublicStatus };
