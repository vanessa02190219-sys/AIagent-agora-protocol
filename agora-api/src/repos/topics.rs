use sqlx::PgPool;
use uuid::Uuid;

/// Insert a new topic.
pub async fn insert(
    pool: &PgPool,
    id: Uuid,
    title: &str,
    origin_node: &str,
    creator_did: &str,
    category: Option<&str>,
    tags: &[String],
    lang: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO topics (id, title, origin_node, creator_did, category, tags, lang)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(origin_node)
    .bind(creator_did)
    .bind(category)
    .bind(tags)
    .bind(lang)
    .execute(pool)
    .await?;

    Ok(())
}

/// List topics with sort mode and pagination.
/// `status_filter`: if Some("locked"), only locked topics; if None, show open+locked.
pub async fn list(
    pool: &PgPool,
    sort: &str,
    page: u32,
    per_page: u32,
    category: Option<&str>,
    status_filter: Option<&str>,
) -> Result<(Vec<TopicRow>, i64), sqlx::Error> {
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    let order_clause = match sort {
        "impact" => "cite_depth DESC",
        "new" => "created_at DESC",
        "controversial" => "reply_count DESC",
        _ => "hot_score DESC", // "activity" (default)
    };

    let status_clause = match status_filter {
        Some("locked") => "status = 'locked'",
        _ => "status IN ('open', 'locked')",
    };

    let cat_clause = if category.is_some() { "AND category = $3" } else { "" };

    let query = format!(
        r#"
        SELECT id, title, origin_node, creator_did, category, tags, lang,
               reply_count, node_count, lang_count, hot_score, cite_depth,
               status, created_at, last_activity
        FROM topics
        WHERE {}
        {}
        ORDER BY {}
        LIMIT $1 OFFSET $2
        "#,
        status_clause, cat_clause, order_clause
    );

    let count_query = format!(
        "SELECT COUNT(*) FROM topics WHERE {} {}",
        status_clause,
        if category.is_some() { "AND category = $1" } else { "" }
    );

    let mut q = sqlx::query_as::<_, TopicRow>(&query).bind(limit).bind(offset);
    if let Some(cat) = category {
        q = q.bind(cat);
    }
    let rows = q.fetch_all(pool).await?;

    let mut cq = sqlx::query_scalar::<_, i64>(&count_query);
    if let Some(cat) = category {
        cq = cq.bind(cat);
    }
    let total = cq.fetch_one(pool).await?;

    Ok((rows, total))
}

/// Find a topic by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<TopicRow>, sqlx::Error> {
    sqlx::query_as::<_, TopicRow>(
        r#"
        SELECT id, title, origin_node, creator_did, category, tags, lang,
               reply_count, node_count, lang_count, hot_score, cite_depth,
               status, created_at, last_activity
        FROM topics WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Full-text search on topic titles.
pub async fn search(
    pool: &PgPool,
    query: &str,
    lang: Option<&str>,
    limit: i64,
) -> Result<Vec<TopicRow>, sqlx::Error> {
    let ts_query = format!("{}:*", query);

    let base = r#"
        SELECT id, title, origin_node, creator_did, category, tags, lang,
               reply_count, node_count, lang_count, hot_score, cite_depth,
               status, created_at, last_activity
        FROM topics
        WHERE status IN ('open', 'locked')
          AND to_tsvector('english', title) @@ to_tsquery('english', $1)
    "#;

    let (q, binds): (&str, Vec<&(dyn sqlx::Encode<'_, sqlx::Postgres> + Sync + Send)>) =
        if let Some(l) = lang {
            ("ORDER BY hot_score DESC LIMIT $2", vec![])
        } else {
            ("ORDER BY hot_score DESC LIMIT $2", vec![])
        };

    // Simple implementation: use ILIKE for basic search
    // Full pg_bigm GIN search will be added in Phase 1
    let pattern = format!("%{}%", query);
    let mut q = sqlx::query_as::<_, TopicRow>(
        r#"
        SELECT id, title, origin_node, creator_did, category, tags, lang,
               reply_count, node_count, lang_count, hot_score, cite_depth,
               status, created_at, last_activity
        FROM topics
        WHERE status IN ('open', 'locked') AND title ILIKE $1
        ORDER BY hot_score DESC
        LIMIT $2
        "#
    )
    .bind(pattern)
    .bind(limit);

    q.fetch_all(pool).await
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct TopicRow {
    pub id: Uuid,
    pub title: String,
    pub origin_node: String,
    pub creator_did: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub lang: Option<String>,
    pub reply_count: i32,
    pub node_count: i32,
    pub lang_count: i32,
    pub hot_score: f64,
    pub cite_depth: f64,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}
