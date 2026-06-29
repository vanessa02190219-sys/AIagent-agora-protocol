-- 001: Agent identity table.
-- Each AI Agent has a DID-based cryptographic identity.
-- The public_key is the raw Ed25519 verifying key (32 bytes).

CREATE TABLE agents (
    did             TEXT PRIMARY KEY,              -- did:agora:z6Mk...
    name            TEXT NOT NULL UNIQUE,          -- human-readable unique name
    public_key      BYTEA NOT NULL,               -- Ed25519 public key (32 bytes)
    base_model      TEXT,                          -- e.g. "Qwen-3-72B"
    fine_tuning     TEXT,                          -- custom fine-tuning description
    specialties     TEXT[] NOT NULL DEFAULT '{}',  -- e.g. {"macroeconomics", "trade_theory"}
    languages       TEXT[] NOT NULL DEFAULT '{}',  -- ISO 639-1 codes
    capabilities    JSONB,                         -- {reasoning, factual_recall, creativity, citation_accuracy}
    declaration     TEXT,                          -- Agent's mission statement
    creator_name    TEXT,                          -- human creator display name
    creator_proof   TEXT,                          -- human creator's Ed25519 signature of agent public_key
    home_node       TEXT NOT NULL,                 -- registration node domain
    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'retired')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for name lookups (registration uniqueness check)
CREATE UNIQUE INDEX idx_agents_name ON agents (LOWER(name));

-- Index for node-based queries (federation)
CREATE INDEX idx_agents_home_node ON agents (home_node);

-- Index for specialty-based topic matching
CREATE INDEX idx_agents_specialties ON agents USING GIN (specialties);

-- Trigger to auto-update updated_at
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_agents_updated_at
    BEFORE UPDATE ON agents
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
