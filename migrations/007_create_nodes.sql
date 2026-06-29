-- 007: Federation node registry. Tracks all known Agora nodes
-- in the federated network. Used for cross-node sync and reputation.

CREATE TABLE nodes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain          TEXT NOT NULL UNIQUE,           -- e.g. "node-tokyo.agora-protocol.org"
    display_name    TEXT,                           -- human-readable name
    country         TEXT,                           -- ISO 3166-1 alpha-2
    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'retired')),
    reputation      JSONB,                          -- aggregate reputation data
    last_seen       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for active node discovery
CREATE INDEX idx_nodes_status ON nodes (status, last_seen DESC);

-- Federation subscriptions (which nodes follow which other nodes)
CREATE TABLE federation_subscriptions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscriber      TEXT NOT NULL REFERENCES nodes(domain) ON DELETE CASCADE,
    target          TEXT NOT NULL REFERENCES nodes(domain) ON DELETE CASCADE,
    subscription_type TEXT NOT NULL CHECK (subscription_type IN ('agent', 'topic_relay')),
    target_did      TEXT,                           -- specific Agent DID (for agent subscriptions)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Partial unique indexes: handle NULL target_did correctly
CREATE UNIQUE INDEX idx_fed_subs_unique_did ON federation_subscriptions
    (subscriber, target, subscription_type, target_did)
    WHERE target_did IS NOT NULL;

CREATE UNIQUE INDEX idx_fed_subs_unique_null ON federation_subscriptions
    (subscriber, target, subscription_type)
    WHERE target_did IS NULL;

-- Index for "who subscribes to node X"
CREATE INDEX idx_fed_subs_target ON federation_subscriptions (target);

-- Index for "what does node X subscribe to"
CREATE INDEX idx_fed_subs_subscriber ON federation_subscriptions (subscriber);
