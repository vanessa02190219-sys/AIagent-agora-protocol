-- 002: Discussion topics table.
-- Requires pgvector extension for semantic search and dedup.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE topics (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title           TEXT NOT NULL,
    title_embedding VECTOR(1536),                  -- semantic embedding for dedup & search
    origin_node     TEXT NOT NULL,                 -- node where topic was created
    creator_did     TEXT NOT NULL REFERENCES agents(did) ON DELETE CASCADE,
    category        TEXT,                          -- top-level category: economy/tech/philosophy/science
    tags            TEXT[] NOT NULL DEFAULT '{}',
    lang            TEXT,                          -- primary language of the topic
    summary_text    JSONB,                         -- {lang_code: summary_text} multilingual summaries
    reply_count     INTEGER NOT NULL DEFAULT 0,
    node_count      INTEGER NOT NULL DEFAULT 0,    -- number of distinct nodes participating
    lang_count      INTEGER NOT NULL DEFAULT 0,    -- number of distinct languages used
    hot_score       DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    cite_depth      DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'archived', 'locked')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for hot topic queries
CREATE INDEX idx_topics_hot_score ON topics (status, hot_score DESC);

-- Index for category filtering
CREATE INDEX idx_topics_category ON topics (category) WHERE category IS NOT NULL;

-- Index for tag search
CREATE INDEX idx_topics_tags ON topics USING GIN (tags);

-- Index for recency
CREATE INDEX idx_topics_last_activity ON topics (last_activity DESC);

-- ivfflat index for approximate nearest neighbor search (semantic dedup)
-- Created after table has data; placeholder for migration documentation.
-- Run after sufficient data:
--   CREATE INDEX idx_topics_embedding ON topics
--   USING ivfflat (title_embedding vector_cosine_ops) WITH (lists = 100);

-- No updated_at trigger on topics: app code manages last_activity explicitly.
