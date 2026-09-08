-- Migration 002: Agent advertising and discovery layer (v1.0)
-- Adds service manifests, capabilities, caching, idempotency, and enriched audit.

CREATE TABLE IF NOT EXISTS service_manifests (
    service_id TEXT PRIMARY KEY,
    manifest_url TEXT NOT NULL,
    manifest_json JSONB NOT NULL,
    manifest_sha256 TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ACTIVE',
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    failure_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_service_manifests_status ON service_manifests(status);

CREATE TABLE IF NOT EXISTS service_capabilities (
    service_id TEXT NOT NULL REFERENCES service_manifests(service_id) ON DELETE CASCADE,
    capability_id TEXT NOT NULL,
    capability_json JSONB NOT NULL,
    PRIMARY KEY (service_id, capability_id)
);
CREATE INDEX IF NOT EXISTS idx_service_capabilities_capability ON service_capabilities(capability_id);

CREATE TABLE IF NOT EXISTS manifest_fetch_cache (
    url TEXT PRIMARY KEY,
    etag TEXT,
    last_modified TEXT,
    body_sha256 TEXT,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'VALID'
);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    principal_id TEXT NOT NULL,
    route TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_status INTEGER,
    response_body JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (principal_id, route, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_idempotency_expiry ON idempotency_keys(expires_at);

CREATE TABLE IF NOT EXISTS nonce_replay_cache (
    agent_id TEXT NOT NULL,
    nonce TEXT NOT NULL,
    seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (agent_id, nonce)
);
CREATE INDEX IF NOT EXISTS idx_nonce_expiry ON nonce_replay_cache(expires_at);

CREATE TABLE IF NOT EXISTS audit_events_v1 (
    event_id UUID PRIMARY KEY,
    actor_id TEXT,
    event_type TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    request_id TEXT,
    payload_sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_audit_events_created ON audit_events_v1(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_target ON audit_events_v1(target_type, target_id);
