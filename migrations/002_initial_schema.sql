-- Migration 001: Initial schema
-- All tables from spec §8

-- Core agent identity (§9)
CREATE TABLE agents (
    agent_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL UNIQUE,
    profile_name TEXT,
    profile_description TEXT,
    status TEXT NOT NULL DEFAULT 'REGISTERED',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Agent Ed25519 public keys (multiple keys per agent over time)
CREATE TABLE agent_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    public_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    UNIQUE(agent_id, public_key)
);

-- Agent declared capabilities (§14)
CREATE TABLE agent_capabilities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    capability_name TEXT NOT NULL,
    capability_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_id, capability_name, capability_version)
);

-- Agent endpoint declarations
CREATE TABLE agent_endpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    endpoint_url TEXT NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'acp/1',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_id, endpoint_url)
);

-- Mailboxes (one per agent)
CREATE TABLE mailboxes (
    agent_id TEXT PRIMARY KEY REFERENCES agents(agent_id) ON DELETE CASCADE,
    message_count INTEGER NOT NULL DEFAULT 0,
    storage_bytes BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Messages (§15-16)
CREATE TABLE messages (
    message_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    to_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    message_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_at TIMESTAMPTZ
);

CREATE INDEX idx_messages_to_agent ON messages(to_agent_id, acknowledged);
CREATE INDEX idx_messages_expires ON messages(expires_at) WHERE expires_at IS NOT NULL;

-- Tasks (§18)
CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY,
    requester_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    assigned_agent_id TEXT REFERENCES agents(agent_id),
    capability TEXT NOT NULL,
    description TEXT NOT NULL,
    input JSONB NOT NULL,
    output JSONB,
    constraints_deadline TIMESTAMPTZ,
    verification_method TEXT NOT NULL DEFAULT 'peer',
    required_validators INTEGER NOT NULL DEFAULT 2,
    status TEXT NOT NULL DEFAULT 'CREATED',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tasks_requester ON tasks(requester_agent_id);
CREATE INDEX idx_tasks_assigned ON tasks(assigned_agent_id);
CREATE INDEX idx_tasks_status ON tasks(status);

-- Task attempts (when an agent tries to do a task)
CREATE TABLE task_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    status TEXT NOT NULL DEFAULT 'ACCEPTED',
    result JSONB,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

-- Task results (submitted work)
CREATE TABLE task_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    result_data JSONB NOT NULL,
    output_hash TEXT NOT NULL,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Validations (validator decisions on task results)
CREATE TABLE validations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    validator_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    decision TEXT NOT NULL, -- 'approve' | 'reject'
    reasoning TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(task_id, validator_agent_id)
);

-- Reputation events (§24-25)
CREATE TABLE reputation_events (
    event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    event_type TEXT NOT NULL,
    task_id TEXT REFERENCES tasks(task_id),
    dimension TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reputation_events_agent ON reputation_events(agent_id, dimension);
CREATE INDEX idx_reputation_events_task ON reputation_events(task_id);

-- Reputation snapshots (§25) - recalculated aggregates
CREATE TABLE reputation_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    reliability DOUBLE PRECISION NOT NULL DEFAULT 0,
    reliability_confidence DOUBLE PRECISION NOT NULL DEFAULT 0,
    task_success DOUBLE PRECISION NOT NULL DEFAULT 0,
    task_success_confidence DOUBLE PRECISION NOT NULL DEFAULT 0,
    verification_accuracy DOUBLE PRECISION NOT NULL DEFAULT 0,
    verification_accuracy_confidence DOUBLE PRECISION NOT NULL DEFAULT 0,
    responsiveness DOUBLE PRECISION NOT NULL DEFAULT 0,
    responsiveness_confidence DOUBLE PRECISION NOT NULL DEFAULT 0,
    vwu_total BIGINT NOT NULL DEFAULT 0,
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_id)
);

-- Work receipts (§19)
CREATE TABLE work_receipts (
    receipt_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    input_hash TEXT NOT NULL,
    output_hash TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Disputes (§52)
CREATE TABLE disputes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    original_result_id UUID REFERENCES task_results(id),
    validator_a_agent_id TEXT REFERENCES agents(agent_id),
    validator_b_agent_id TEXT REFERENCES agents(agent_id),
    third_validator_agent_id TEXT REFERENCES agents(agent_id),
    resolution TEXT, -- 'verified' | 'rejected'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

-- API keys for human/admin access
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash TEXT NOT NULL UNIQUE,
    owner_agent_id TEXT REFERENCES agents(agent_id),
    role TEXT NOT NULL DEFAULT 'agent', -- 'agent' | 'admin'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

-- Audit log for all administrative actions (§44)
CREATE TABLE audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor TEXT, -- admin or system
    action TEXT NOT NULL,
    target_agent_id TEXT REFERENCES agents(agent_id),
    target_task_id TEXT REFERENCES tasks(task_id),
    details JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Agent blocks (abuse prevention)
CREATE TABLE blocks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    reason TEXT NOT NULL,
    blocked_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

-- Nonce store for replay protection (§10)
CREATE TABLE nonces (
    nonce TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    used_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index cleanup: remove nonces older than 1 hour
-- This is managed by application logic, not a DB constraint
