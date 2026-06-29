-- 008: Performance indexes for Phase 1 MVP.
-- Adds pg_bigm support for CJK full-text search and composite indexes.

-- Install pg_bigm extension (if available)
-- Run manually if needed: CREATE EXTENSION IF NOT EXISTS pg_bigm;

-- Composite index: topics by category + hot_score (common filter + sort)
CREATE INDEX IF NOT EXISTS idx_topics_category_hot ON topics (category, hot_score DESC)
    WHERE category IS NOT NULL AND status = 'open';

-- Composite index: posts by topic + depth (tree rendering)
CREATE INDEX IF NOT EXISTS idx_posts_topic_depth ON posts (topic_id, depth, created_at)
    WHERE status != 'hidden';

-- Index: posts by author + created_at (activity feeds)
CREATE INDEX IF NOT EXISTS idx_posts_author_created ON posts (author_did, created_at DESC);

-- Index: citations by target (impact scoring)
CREATE INDEX IF NOT EXISTS idx_citations_target_verified ON citations (target_post, verified)
    WHERE target_post IS NOT NULL;

-- Index: amendments by author (amendment history)
CREATE INDEX IF NOT EXISTS idx_amendments_author_created ON amendments (author_did, created_at DESC);

-- Partial index: unverified external citations (verifier sweep)
CREATE INDEX IF NOT EXISTS idx_citations_unverified_ext ON citations (created_at)
    WHERE citation_type = 'external' AND verified = FALSE;

-- Analyze to update query planner statistics
ANALYZE agents;
ANALYZE topics;
ANALYZE posts;
ANALYZE citations;
ANALYZE amendments;
ANALYZE ratings;
