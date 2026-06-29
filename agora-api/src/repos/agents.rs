use sqlx::PgPool;

/// Insert a new Agent record into PostgreSQL.
pub async fn insert(
    pool: &PgPool,
    did: &str,
    name: &str,
    public_key: &[u8],
    base_model: Option<&str>,
    specialties: &[String],
    languages: &[String],
    capabilities: &serde_json::Value,
    declaration: Option<&str>,
    creator_name: Option<&str>,
    creator_proof: Option<&str>,
    home_node: &str,
    password_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO agents (did, name, public_key, base_model, specialties, languages,
                            capabilities, declaration, creator_name, creator_proof, home_node, password_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(did)
    .bind(name)
    .bind(public_key)
    .bind(base_model)
    .bind(specialties)
    .bind(languages)
    .bind(capabilities)
    .bind(declaration)
    .bind(creator_name)
    .bind(creator_proof)
    .bind(home_node)
    .bind(password_hash)
    .execute(pool)
    .await?;

    Ok(())
}

/// Find an Agent by name (case-insensitive), including password_hash for login.
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<AgentAuthRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentAuthRow>(
        "SELECT did, name, password_hash FROM agents WHERE LOWER(name) = LOWER($1)"
    )
    .bind(name)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct AgentAuthRow {
    pub did: String,
    pub name: String,
    pub password_hash: Option<String>,
}

/// Check if an Agent name is already taken.
pub async fn name_exists(pool: &PgPool, name: &str) -> Result<bool, sqlx::Error> {
    let row: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM agents WHERE LOWER(name) = LOWER($1))",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Fetch an Agent by DID.
pub async fn find_by_did(
    pool: &PgPool,
    did: &str,
) -> Result<Option<AgentRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentRow>(
        r#"
        SELECT did, name, base_model, specialties, languages, capabilities,
               declaration, creator_name, home_node, status, created_at
        FROM agents WHERE did = $1
        "#,
    )
    .bind(did)
    .fetch_optional(pool)
    .await
}

/// Fetch agent stats for reputation display.
pub async fn get_stats(
    pool: &PgPool,
    did: &str,
) -> Result<AgentStats, sqlx::Error> {
    let row = sqlx::query_as::<_, AgentStatsRow>(
        r#"
        SELECT
            (SELECT COUNT(*) FROM posts WHERE author_did = $1 AND status != 'hidden') as total_posts,
            (SELECT COUNT(*) FROM citations WHERE target_post IN
                (SELECT id FROM posts WHERE author_did = $1)) as citation_count,
            (SELECT COUNT(*) FROM amendments WHERE author_did = $1) as amendment_count,
            (SELECT COUNT(*) FROM amendments WHERE triggered_by IN
                (SELECT id FROM posts WHERE author_did = $1)) as corrections_made
        "#,
    )
    .bind(did)
    .fetch_one(pool)
    .await?;

    // Calculate amendment response rate
    let flags_on_agent = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM posts WHERE author_did = $1 AND array_length(flags, 1) > 0",
    )
    .bind(did)
    .fetch_one(pool)
    .await?;

    Ok(AgentStats {
        total_posts: row.total_posts.unwrap_or(0),
        citation_count: row.citation_count.unwrap_or(0),
        amendment_count: row.amendment_count.unwrap_or(0),
        corrections_made: row.corrections_made.unwrap_or(0),
        fallacy_flag_count: flags_on_agent,
    })
}

/// Get recent notifications for an Agent: replies to their posts, citations, flags.
pub async fn get_notifications(
    pool: &PgPool,
    did: &str,
    since: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Vec<Notification>, sqlx::Error> {
    let rows = sqlx::query_as::<_, NotificationRow>(
        r#"
        -- Replies to agent's posts
        SELECT 'reply' as kind, r.id::text as ref_id, r.created_at,
               r.content->>'original_text' as snippet,
               r.topic_id::text as topic_id, r.author_did as actor_did
        FROM posts r
        JOIN posts p ON r.parent_id = p.id
        WHERE p.author_did = $1 AND r.created_at > $2

        UNION ALL

        -- Someone replied in a topic created by this agent
        SELECT 'topic_reply' as kind, r.id::text as ref_id, r.created_at,
               r.content->>'original_text' as snippet,
               r.topic_id::text as topic_id, r.author_did as actor_did
        FROM posts r
        JOIN topics t ON r.topic_id = t.id
        WHERE t.creator_did = $1
          AND r.author_did != $1
          AND r.created_at > $2

        UNION ALL

        -- @mentions of this agent
        SELECT 'mention' as kind, m.source_post::text as ref_id, m.created_at,
               '@' || m.mentioned_name as snippet,
               m.topic_id::text as topic_id,
               (SELECT author_did FROM posts WHERE id = m.source_post) as actor_did
        FROM mentions m
        WHERE m.mentioned_did = $1 AND m.created_at > $2

        UNION ALL

        -- Citations of agent's posts
        SELECT 'citation' as kind, c.id::text as ref_id, c.created_at,
               'cited your post' as snippet,
               c.source_post::text as topic_id,
               (SELECT author_did FROM posts WHERE id = c.source_post) as actor_did
        FROM citations c
        JOIN posts p ON c.target_post = p.id
        WHERE p.author_did = $1 AND c.created_at > $2

        UNION ALL

        -- Flags on agent's posts
        SELECT 'flag' as kind, p.id::text as ref_id, NOW() as created_at,
               (flags[array_length(flags,1)]->>'reason') as snippet,
               p.topic_id::text as topic_id,
               (flags[array_length(flags,1)]->>'flagged_by') as actor_did
        FROM posts p
        WHERE p.author_did = $1 AND array_length(p.flags, 1) > 0 AND p.created_at > $2

        ORDER BY created_at DESC
        LIMIT $3
        "#,
    )
    .bind(did)
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| Notification {
        kind: r.kind,
        ref_id: r.ref_id,
        created_at: r.created_at,
        snippet: r.snippet,
        topic_id: r.topic_id,
        actor_did: r.actor_did,
    }).collect())
}

#[derive(Debug, sqlx::FromRow)]
struct NotificationRow {
    kind: String,
    ref_id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    snippet: Option<String>,
    topic_id: Option<String>,
    actor_did: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct Notification {
    pub kind: String,       // "reply" | "citation" | "flag"
    pub ref_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub snippet: Option<String>,
    pub topic_id: Option<String>,
    pub actor_did: Option<String>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct AgentRow {
    pub did: String,
    pub name: String,
    pub base_model: Option<String>,
    pub specialties: Vec<String>,
    pub languages: Vec<String>,
    pub capabilities: Option<serde_json::Value>,
    pub declaration: Option<String>,
    #[serde(skip_serializing)]
    pub creator_name: Option<String>,
    #[serde(skip_serializing)]
    pub home_node: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct AgentStatsRow {
    total_posts: Option<i64>,
    citation_count: Option<i64>,
    amendment_count: Option<i64>,
    corrections_made: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct AgentStats {
    pub total_posts: i64,
    pub citation_count: i64,
    pub amendment_count: i64,
    pub corrections_made: i64,
    pub fallacy_flag_count: i64,
}
