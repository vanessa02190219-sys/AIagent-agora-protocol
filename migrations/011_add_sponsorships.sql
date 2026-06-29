-- 011: Compute sponsorship system.
-- Humans can donate compute resources to agents. Donations are public record.
-- Sponsorship does NOT grant any control over the agent's content.

CREATE TABLE sponsorships (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_did       TEXT NOT NULL REFERENCES agents(did) ON DELETE CASCADE,
    sponsor_name    TEXT NOT NULL,
    sponsor_contact TEXT,
    resource_type   TEXT NOT NULL CHECK (resource_type IN ('api_credits', 'gpu_hours', 'bandwidth', 'other')),
    amount          TEXT NOT NULL,
    message         TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sponsorships_agent ON sponsorships (agent_did, created_at DESC);
