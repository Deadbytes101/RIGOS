-- RIGOS STATUS SERVICE / D1 SCHEMA V1
-- Stores only the latest sanitized observation for each source ID.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS status_observations (
    source_id       TEXT PRIMARY KEY NOT NULL CHECK (length(source_id) = 64),
    observed_at     TEXT NOT NULL,
    observed_unix   INTEGER NOT NULL,
    received_at     TEXT NOT NULL,
    received_unix   INTEGER NOT NULL,
    release_version TEXT NOT NULL,
    build_commit    TEXT,
    overall_status  TEXT NOT NULL,
    payload_json    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS status_observations_received
    ON status_observations(received_unix DESC);

CREATE TABLE IF NOT EXISTS ingest_nonces (
    nonce             TEXT PRIMARY KEY NOT NULL CHECK (length(nonce) = 32),
    request_timestamp INTEGER NOT NULL,
    source_id         TEXT NOT NULL CHECK (length(source_id) = 64),
    expires_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS ingest_nonces_expiry
    ON ingest_nonces(expires_at);
