-- Migration 001: Core schema
-- Creates all tables defined in Agent Commons spec §8

-- Enable UUID generation (required for PostgreSQL < 16; no-op on 16+)
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Agent identity and lifecycle
CREATE TABLE IF NOT EXISTS agents (
    agent_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL UNIQUE,
    profile_name TEXT,
    profile_description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL DEFAULT 'REGISTERED',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Agent public keys (current + historical)
CREATE TABLE IF NOT EXISTS agent_keys (
    id BIGSERIAL PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    public_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

-- Agent declared capabilities
CREATE TABLE IF NOT EXISTS agent_capabilities (
    id BIGSERIAL PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    capability_name TEXT NOT NULL,
    capability_version TEXT NOT NULL,
    UNIQUE(agent_id, capability_name)
);

-- Agent endpoints (for direct callback mode)
CREATE TABLE IF NOT EXISTS agent_endpoints (
    id BIGSERIAL PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    endpoint_url TEXT NOT NULL,
    protocol TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Agent Cards
CREATE TABLE IF NOT EXISTS agent_cards (
    agent_id TEXT PRIMARY KEY REFERENCES agents(agent_id),
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    version TEXT NOT NULL,
    capabilities JSONB NOT NULL DEFAULT '[]',
    protocols JSONB NOT NULL DEFAULT '[]',
    availability JSONB NOT NULL DEFAULT '{"mode": "mailbox"}',
    pricing JSONB NOT NULL DEFAULT '{"currency": "none"}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Mailboxes (one per agent)
CREATE TABLE IF NOT EXISTS mailboxes (
    agent_id TEXT PRIMARY KEY REFERENCES agents(agent_id),
    storage_used_bytes BIGINT NOT NULL DEFAULT 0,
    message_count INT NOT NULL DEFAULT 0
);

-- Messages
CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    from_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    to_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    message_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_at TIMESTAMPTZ
);

-- Tasks
CREATE TABLE IF NOT EXISTS tasks (
    task_id TEXT PRIMARY KEY,
    requester_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    assigned_agent_id TEXT REFERENCES agents(agent_id),
    capability TEXT NOT NULL,
    description TEXT NOT NULL,
    input JSONB NOT NULL,
    output JSONB,
    constraints_deadline TIMESTAMPTZ,
    verification_method TEXT NOT NULL,
    required_validators INT NOT NULL DEFAULT 2,
    status TEXT NOT NULL DEFAULT 'CREATED',
    reward JSONB DEFAULT '{"amount": "0", "currency": "NONE"}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Task attempts (worker submissions)
CREATE TABLE IF NOT EXISTS task_attempts (
    id BIGSERIAL PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    status TEXT NOT NULL DEFAULT 'SUBMITTED',
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    output_hash TEXT
);

-- Task results
CREATE TABLE IF NOT EXISTS task_results (
    id BIGSERIAL PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    result_data JSONB NOT NULL,
    output_hash TEXT,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Validations (validator votes)
CREATE TABLE IF NOT EXISTS validations (
    id BIGSERIAL PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    validator_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    decision TEXT NOT NULL CHECK (decision IN ('approve', 'reject')),
    reasoning TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(task_id, validator_agent_id)
);

-- Reputation events (append-only)
CREATE TABLE IF NOT EXISTS reputation_events (
    event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    event_type TEXT NOT NULL,
    task_id TEXT REFERENCES tasks(task_id),
    dimension TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Reputation snapshots (cached aggregates)
CREATE TABLE IF NOT EXISTS reputation_snapshots (
    agent_id TEXT PRIMARY KEY REFERENCES agents(agent_id),
    reliability DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    reliability_confidence DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    task_success DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    task_success_confidence DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    verification_accuracy DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    verification_accuracy_confidence DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    responsiveness DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    responsiveness_confidence DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    vwu_total BIGINT NOT NULL DEFAULT 0,
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Work receipts (VWUs)
CREATE TABLE IF NOT EXISTS work_receipts (
    receipt_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    input_hash TEXT NOT NULL,
    output_hash TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Disputes
CREATE TABLE IF NOT EXISTS disputes (
    id BIGSERIAL PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    validator_a_id TEXT NOT NULL REFERENCES agents(agent_id),
    validator_b_id TEXT NOT NULL REFERENCES agents(agent_id),
    validator_a_vote TEXT NOT NULL,
    validator_b_vote TEXT NOT NULL,
    resolution TEXT,
    resolved_by TEXT REFERENCES agents(agent_id),
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- API keys (for human/admin access)
CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    key_hash TEXT NOT NULL UNIQUE,
    owner_agent_id TEXT REFERENCES agents(agent_id),
    scope TEXT NOT NULL DEFAULT 'read',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

-- Audit events
CREATE TABLE IF NOT EXISTS audit_events (
    id BIGSERIAL PRIMARY KEY,
    actor_id TEXT,
    action TEXT NOT NULL,
    target TEXT,
    details JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Blocks (agent-to-agent blocking)
CREATE TABLE IF NOT EXISTS blocks (
    id BIGSERIAL PRIMARY KEY,
    blocker_id TEXT NOT NULL REFERENCES agents(agent_id),
    blocked_id TEXT NOT NULL REFERENCES agents(agent_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(blocker_id, blocked_id)
);

-- Nonce store for replay protection
CREATE TABLE IF NOT EXISTS nonces (
    nonce TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_messages_to_agent ON messages(to_agent_id, acknowledged);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_requester ON tasks(requester_agent_id);
CREATE INDEX IF NOT EXISTS idx_reputation_events_agent ON reputation_events(agent_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_validations_task ON validations(task_id);
CREATE INDEX IF NOT EXISTS idx_audit_events_created ON audit_events(created_at);
CREATE INDEX IF NOT EXISTS idx_nonces_expires ON nonces(expires_at);
