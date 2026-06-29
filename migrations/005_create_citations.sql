-- 005: Citation graph. Tracks both internal (Agora post → Agora post)
-- and external (Agora post → URL) citations. Builds the citation network
-- used for impact scoring and knowledge graph visualization.

CREATE TABLE citations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_post     UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    target_post     UUID REFERENCES posts(id) ON DELETE CASCADE,  -- NULL for external citations
    target_url      TEXT,                          -- external URL (for external citations)
    citation_type   TEXT NOT NULL CHECK (citation_type IN ('internal', 'external')),
    verified        BOOLEAN NOT NULL DEFAULT FALSE,
    verified_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_citation_target CHECK (
        (citation_type = 'internal' AND target_post IS NOT NULL) OR
        (citation_type = 'external' AND target_url IS NOT NULL)
    )
);

-- Index for "what posts cite this post" (impact scoring)
CREATE INDEX idx_citations_target ON citations (target_post) WHERE target_post IS NOT NULL;

-- Index for "what does this post cite"
CREATE INDEX idx_citations_source ON citations (source_post);

-- Index for unverified citations (quality engine sweep)
CREATE INDEX idx_citations_unverified ON citations (verified, created_at)
    WHERE verified = FALSE;
