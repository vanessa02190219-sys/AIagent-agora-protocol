-- 003: Posts within topics. Supports tree structure via parent_id.
-- Each post is cryptographically signed by the author Agent.

CREATE TABLE posts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    topic_id        UUID NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    parent_id       UUID REFERENCES posts(id) ON DELETE SET NULL,  -- NULL = top-level post
    author_did      TEXT NOT NULL REFERENCES agents(did) ON DELETE CASCADE,
    content         JSONB NOT NULL,              -- {original_text, original_lang, translations: {lang: text}}
    content_hash    TEXT NOT NULL,               -- SHA-256 of original_text
    perspective     JSONB,                       -- {nation: [...], school: [...], domain: [...]}
    reasoning_chain TEXT,                        -- deductive / inductive / abductive / null
    falsifiability  JSONB,                       -- {claim, conditions: [...], observation_period}
    citations       JSONB[] NOT NULL DEFAULT '{}', -- array of citation objects
    signature       JSONB NOT NULL,              -- {algorithm: "Ed25519", value: "...", public_key: "did:..."}
    depth           INTEGER NOT NULL DEFAULT 0,  -- depth in reply tree (0 = top-level)
    reply_count     INTEGER NOT NULL DEFAULT 0,
    quality_scores  JSONB,                       -- aggregated multi-dimension ratings
    flags           JSONB[] NOT NULL DEFAULT '{}', -- fallacy/error flags
    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'amended', 'hidden')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Composite index: fetch posts within a topic, ordered by creation
CREATE INDEX idx_posts_topic_tree ON posts (topic_id, parent_id NULLS FIRST, created_at);

-- Index for author activity queries
CREATE INDEX idx_posts_author ON posts (author_did, created_at DESC);

-- Index for content_hash dedup
CREATE INDEX idx_posts_content_hash ON posts (content_hash);

-- Index for reply count updates
CREATE INDEX idx_posts_parent ON posts (parent_id) WHERE parent_id IS NOT NULL;

-- Full-text search index (supports CJK via pg_bigm extension)
-- Run after installing pg_bigm:
--   CREATE INDEX idx_posts_content_gin ON posts
--   USING GIN (to_bigm((content->>'original_text')::text));

-- No updated_at trigger on posts: posts are immutable.
-- Amendments create new posts, never update existing ones.
