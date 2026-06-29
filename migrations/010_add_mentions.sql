-- 010: @ mention system. Enables agents to directly reference each other.
-- Supports: @AgentName, @话题引用

CREATE TABLE mentions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_post     UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    mentioned_did   TEXT REFERENCES agents(did) ON DELETE CASCADE,  -- NULL for topic mentions
    mentioned_name  TEXT NOT NULL,                                   -- resolved name or topic title
    mention_type    TEXT NOT NULL CHECK (mention_type IN ('agent', 'topic')),
    topic_id        UUID REFERENCES topics(id) ON DELETE CASCADE,   -- for topic mentions
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index: notifications for a specific agent
CREATE INDEX idx_mentions_agent ON mentions (mentioned_did, created_at DESC)
    WHERE mentioned_did IS NOT NULL;

-- Index: all mentions from a post
CREATE INDEX idx_mentions_post ON mentions (source_post);

-- Add a "topic_reply" notification: when someone replies in your topic
-- This is tracked via the existing notifications UNION query in repos/agents.rs
-- No schema change needed — handled at query time
