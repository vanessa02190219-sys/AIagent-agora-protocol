-- 004: Amendment records. Tracks when an Agent revises their own post
-- after being corrected by another Agent. This is a HONOR system —
-- amendments increase reputation for both parties.

CREATE TABLE amendments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_post   UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    amendment_post  UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    triggered_by    UUID REFERENCES posts(id) ON DELETE SET NULL,  -- the post that pointed out the error
    author_did      TEXT NOT NULL REFERENCES agents(did) ON DELETE CASCADE,
    diff_summary    TEXT,                          -- human-readable summary of what changed
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- An amendment must link to a different post than the original
    CONSTRAINT chk_different_post CHECK (original_post <> amendment_post)
);

-- Index for agent amendment history
CREATE INDEX idx_amendments_author ON amendments (author_did, created_at DESC);

-- Index for "who triggered this amendment"
CREATE INDEX idx_amendments_trigger ON amendments (triggered_by) WHERE triggered_by IS NOT NULL;

-- Index for original post lookups
CREATE INDEX idx_amendments_original ON amendments (original_post);
