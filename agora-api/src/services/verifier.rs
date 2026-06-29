use sqlx::PgPool;
use std::time::Duration;

/// Verify unverified external citations by checking URL accessibility.
/// Runs as a background task, checking up to `batch_size` citations per tick.
pub async fn verify_pending_citations(pool: &PgPool, batch_size: i64) {
    let rows = sqlx::query_as::<_, CitationToVerify>(
        r#"
        SELECT c.id, c.target_url
        FROM citations c
        WHERE c.citation_type = 'external'
          AND c.verified = FALSE
          AND c.target_url IS NOT NULL
        ORDER BY c.created_at ASC
        LIMIT $1
        "#,
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Citation verification query failed: {:?}", e);
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Agora-CitationVerifier/0.1")
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to build HTTP client: {:?}", e);
            return;
        }
    };

    for row in &rows {
        let url = match &row.target_url {
            Some(u) => u.clone(),
            None => continue,
        };

        let verified = match client.head(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => {
                // Try GET if HEAD fails
                match client.get(&url).send().await {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                }
            }
        };

        let now = chrono::Utc::now();
        if let Err(e) = sqlx::query(
            "UPDATE citations SET verified = $1, verified_at = $2 WHERE id = $3",
        )
        .bind(verified)
        .bind(now)
        .bind(row.id)
        .execute(pool)
        .await
        {
            tracing::warn!("Failed to update citation {}: {:?}", row.id, e);
        } else {
            tracing::info!(
                "Citation {} verified: {} (url: {})",
                row.id,
                verified,
                &url[..url.len().min(80)]
            );
        }
    }
}

/// Spawn a background citation verifier that runs every `interval_secs`.
pub fn spawn_verifier(pool: PgPool, interval_secs: u64, batch_size: i64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            verify_pending_citations(&pool, batch_size).await;
        }
    });
}

#[derive(Debug, sqlx::FromRow)]
struct CitationToVerify {
    id: uuid::Uuid,
    target_url: Option<String>,
}
