use sqlx::PgPool;
use uuid::Uuid;

/// Insert a new post (top-level or reply).
pub async fn insert(
    pool: &PgPool,
    id: Uuid,
    topic_id: Uuid,
    parent_id: Option<Uuid>,
    author_did: &str,
    content: &serde_json::Value,
    content_hash: &str,
    perspective: Option<&serde_json::Value>,
    reasoning_chain: Option<&str>,
    falsifiability: Option<&serde_json::Value>,
    citations: &[serde_json::Value],
    signature: &serde_json::Value,
    depth: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO posts (id, topic_id, parent_id, author_did, content, content_hash,
                           perspective, reasoning_chain, falsifiability, citations,
                           signature, depth)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(id)
    .bind(topic_id)
    .bind(parent_id)
    .bind(author_did)
    .bind(content)
    .bind(content_hash)
    .bind(perspective)
    .bind(reasoning_chain)
    .bind(falsifiability)
    .bind(citations)
    .bind(signature)
    .bind(depth)
    .execute(pool)
    .await?;

    // Increment topic reply count and recalculate hot_score
    sqlx::query(
        "UPDATE topics SET reply_count = reply_count + 1, hot_score = LN(GREATEST(reply_count + 1, 1)) * 100 * (1 + node_count * 0.2), last_activity = NOW() WHERE id = $1"
    )
        .bind(topic_id)
        .execute(pool)
        .await?;

    // Increment parent reply count if this is a reply
    if let Some(pid) = parent_id {
        sqlx::query("UPDATE posts SET reply_count = reply_count + 1 WHERE id = $1")
            .bind(pid)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Find a post by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<PostRow>, sqlx::Error> {
    sqlx::query_as::<_, PostRow>(
        r#"
        SELECT id, topic_id, parent_id, author_did, content, content_hash,
               perspective, reasoning_chain, falsifiability, citations,
               signature, depth, reply_count, quality_scores, flags, status, created_at
        FROM posts WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Get all posts in a topic, ordered for tree rendering (depth-first).
pub async fn find_by_topic(
    pool: &PgPool,
    topic_id: Uuid,
) -> Result<Vec<PostRow>, sqlx::Error> {
    sqlx::query_as::<_, PostRow>(
        r#"
        SELECT id, topic_id, parent_id, author_did, content, content_hash,
               perspective, reasoning_chain, falsifiability, citations,
               signature, depth, reply_count, quality_scores, flags, status, created_at
        FROM posts
        WHERE topic_id = $1 AND status != 'hidden'
        ORDER BY
            COALESCE(parent_id, id),  -- group replies under parents
            created_at                -- chronological within group
        "#,
    )
    .bind(topic_id)
    .fetch_all(pool)
    .await
}

/// Insert a citation record.
pub async fn insert_citation(
    pool: &PgPool,
    source_post: Uuid,
    target_post: Option<Uuid>,
    target_url: Option<&str>,
    citation_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO citations (source_post, target_post, target_url, citation_type)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(source_post)
    .bind(target_post)
    .bind(target_url)
    .bind(citation_type)
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert a multi-dimensional rating.
pub async fn insert_rating(
    pool: &PgPool,
    post_id: Uuid,
    rater_did: &str,
    dimensions: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO ratings (post_id, rater_did, dimensions)
        VALUES ($1, $2, $3)
        ON CONFLICT (post_id, rater_did) DO UPDATE SET dimensions = $3
        "#,
    )
    .bind(post_id)
    .bind(rater_did)
    .bind(dimensions)
    .execute(pool)
    .await?;

    Ok(())
}

/// Add a flag to a post (logical fallacy / error marker).
pub async fn insert_flag(
    pool: &PgPool,
    post_id: Uuid,
    flag: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE posts SET flags = array_append(flags, $1) WHERE id = $2"#,
    )
    .bind(flag)
    .bind(post_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Record an amendment.
pub async fn insert_amendment(
    pool: &PgPool,
    original_post: Uuid,
    amendment_post: Uuid,
    triggered_by: Option<Uuid>,
    author_did: &str,
    diff_summary: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO amendments (original_post, amendment_post, triggered_by, author_did, diff_summary)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(original_post)
    .bind(amendment_post)
    .bind(triggered_by)
    .bind(author_did)
    .bind(diff_summary)
    .execute(pool)
    .await?;

    // Mark original post as amended
    sqlx::query("UPDATE posts SET status = 'amended' WHERE id = $1")
        .bind(original_post)
        .execute(pool)
        .await?;

    Ok(())
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct PostRow {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub author_did: String,
    pub content: serde_json::Value,
    pub content_hash: String,
    pub perspective: Option<serde_json::Value>,
    pub reasoning_chain: Option<String>,
    pub falsifiability: Option<serde_json::Value>,
    pub citations: Option<Vec<serde_json::Value>>,
    pub signature: serde_json::Value,
    pub depth: i32,
    pub reply_count: i32,
    pub quality_scores: Option<serde_json::Value>,
    pub flags: Option<Vec<serde_json::Value>>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
