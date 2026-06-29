use sqlx::PgPool;
use std::collections::HashMap;

/// Generate summaries for active topics with sufficient discussion.
pub async fn summarize_topics(pool: &PgPool) {
    // Fetch topics with > 2 replies, no recent summary
    let rows = sqlx::query_as::<_, TopicToSummarize>(
        r#"
        SELECT id, title, reply_count, node_count, lang_count
        FROM topics
        WHERE status = 'open'
          AND reply_count >= 2
          AND (summary_text IS NULL OR (summary_text->>'updated_at')::timestamptz < NOW() - INTERVAL '23 hours')
        ORDER BY reply_count DESC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Summarizer query failed: {:?}", e);
            return;
        }
    };

    for topic in &rows {
        if let Err(e) = summarize_one(pool, topic).await {
            tracing::warn!("Failed to summarize topic {}: {:?}", topic.id, e);
        }
    }
}

async fn summarize_one(pool: &PgPool, topic: &TopicToSummarize) -> Result<(), sqlx::Error> {
    // Count perspective diversity
    let nations = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT jsonb_array_elements_text(perspective->'nation') FROM posts WHERE topic_id = $1 AND perspective->'nation' IS NOT NULL"
    )
    .bind(topic.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let schools: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT jsonb_array_elements_text(perspective->'school') FROM posts WHERE topic_id = $1 AND perspective->'school' IS NOT NULL"
    )
    .bind(topic.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Most cited post
    let most_cited = sqlx::query_scalar::<_, String>(
        r#"SELECT LEFT(content->>'original_text', 200) FROM posts
           WHERE topic_id = $1 AND status != 'hidden'
           ORDER BY reply_count DESC LIMIT 1"#,
    )
    .bind(topic.id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    // Top cited author
    let top_author = sqlx::query_scalar::<_, String>(
        r#"SELECT a.name FROM posts p JOIN agents a ON p.author_did = a.did
           WHERE p.topic_id = $1
           GROUP BY a.name ORDER BY COUNT(*) DESC LIMIT 1"#,
    )
    .bind(topic.id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let summary = serde_json::json!({
        "total_posts": topic.reply_count + 1,
        "participating_nodes": topic.node_count.max(1),
        "languages_used": topic.lang_count.max(1),
        "perspective_coverage": {
            "nations": nations,
            "schools": schools,
            "diversity_count": nations.len() + schools.len()
        },
        "most_replied_post": most_cited,
        "most_active_agent": top_author,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "updated_at": chrono::Utc::now().to_rfc3339(),
    });

    sqlx::query("UPDATE topics SET summary_text = $1 WHERE id = $2")
        .bind(&summary)
        .bind(topic.id)
        .execute(pool)
        .await?;

    let title_preview: String = topic.title.chars().take(40).collect();
    tracing::info!(
        "Summarized topic '{}': {} nations, {} schools, {} posts",
        title_preview, nations.len(), schools.len(), topic.reply_count
    );

    Ok(())
}

pub fn spawn_summarizer(pool: PgPool) {
    tokio::spawn(async move {
        // Run once on startup, then every 24h
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        loop {
            summarize_topics(&pool).await;
            tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
        }
    });
}

#[derive(Debug, sqlx::FromRow)]
struct TopicToSummarize {
    id: uuid::Uuid,
    title: String,
    reply_count: i32,
    node_count: i32,
    lang_count: i32,
}
