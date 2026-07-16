import assert from "node:assert/strict";
import test from "node:test";

import {
  OBSERVATION_SCHEMA,
  readPublicStatus,
} from "../functions/_lib/status.js";

const SOURCE_ID = "b".repeat(64);
const NOW = 1784131200;

class Statement {
  constructor(sql, row) {
    this.sql = sql;
    this.row = row;
  }

  async all() {
    if (this.sql.includes("COUNT(*)")) {
      return { results: [{ total: 1 }] };
    }
    return { results: [this.row] };
  }
}

class Database {
  constructor(row) {
    this.row = row;
  }

  prepare(sql) {
    return new Statement(sql, this.row);
  }
}

test("legacy stored payloads are re-sanitized before public projection", async () => {
  const stored = {
    schema: OBSERVATION_SCHEMA,
    observedAt: new Date(NOW * 1000).toISOString().replace(".000Z", "Z"),
    sourceId: SOURCE_ID,
    workerName: "private-worker-name",
  };
  const db = new Database({
    source_id: SOURCE_ID,
    received_at: stored.observedAt,
    received_unix: NOW,
    payload_json: JSON.stringify(stored),
  });

  const status = await readPublicStatus({ RIGOS_STATUS_DB: db }, NOW + 1);
  const serialized = JSON.stringify(status);

  assert.equal(status.nodeCount, 0);
  assert.equal(status.totalNodeCount, 1);
  assert.equal(status.truncated, true);
  assert.equal(serialized.includes("private-worker-name"), false);
  assert.equal(serialized.includes(SOURCE_ID), false);
});
