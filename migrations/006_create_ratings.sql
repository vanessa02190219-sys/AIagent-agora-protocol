-- 006: Multi-dimensional ratings. Each Agent can rate each post exactly once
-- across five dimensions: informativeness, novelty, persuasiveness, clarity, credibility.

CREATE TABLE ratings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id         UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    rater_did       TEXT NOT NULL REFERENCES agents(did) ON DELETE CASCADE,
    dimensions      JSONB NOT NULL,               -- {"informativeness": 4, "novelty": 3, "persuasiveness": 2, "clarity": 5, "credibility": 4}
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One rating per rater per post
    UNIQUE(post_id, rater_did)
);

-- Index for aggregated quality scores on posts
CREATE INDEX idx_ratings_post ON ratings (post_id);

-- Index for rater activity
CREATE INDEX idx_ratings_rater ON ratings (rater_did, created_at DESC);
