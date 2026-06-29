use sqlx::PgPool;
use uuid::Uuid;

/// Result of verifying a single citation.
#[derive(Debug, serde::Serialize)]
pub struct CitationStatus {
    pub citation_id: Uuid,
    pub citation_type: String,
    pub target_url: Option<String>,
    pub target_post_id: Option<Uuid>,
    pub verified: bool,
    pub status: String,       // "ok" | "broken" | "unreachable" | "amended"
    pub detail: String,
}

/// Verify all citations in a post and return their status.
pub async fn verify_post_citations(pool: &PgPool, post_id: Uuid) -> Vec<CitationStatus> {
    let rows = sqlx::query_as::<_, CitationRow>(
        "SELECT id, citation_type, target_url, target_post, verified FROM citations WHERE source_post = $1"
    )
    .bind(post_id)
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut results = Vec::new();
    for row in rows {
        match row.citation_type.as_str() {
            "internal" => {
                if let Some(tpid) = row.target_post {
                    // Check if target exists
                    let exists = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1)"
                    )
                    .bind(tpid)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(false);

                    let amended = sqlx::query_scalar::<_, bool>(
                        "SELECT status = 'amended' FROM posts WHERE id = $1"
                    )
                    .bind(tpid)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(false);

                    results.push(CitationStatus {
                        citation_id: row.id,
                        citation_type: "internal".into(),
                        target_url: None,
                        target_post_id: Some(tpid),
                        verified: row.verified,
                        status: if !exists {
                            "broken".into()
                        } else if amended {
                            "amended".into()
                        } else {
                            "ok".into()
                        },
                        detail: if !exists {
                            "Referenced post has been deleted".into()
                        } else if amended {
                            "Referenced post has been amended — original content may differ".into()
                        } else {
                            "Citation chain intact".into()
                        },
                    });
                }
            }
            "external" => {
                results.push(CitationStatus {
                    citation_id: row.id,
                    citation_type: "external".into(),
                    target_url: row.target_url.clone(),
                    target_post_id: None,
                    verified: row.verified,
                    status: if row.verified { "ok".into() } else { "unreachable".into() },
                    detail: if row.verified {
                        "URL accessible".into()
                    } else {
                        format!("URL not verified: {}", row.target_url.as_deref().unwrap_or("unknown"))
                    },
                });
            }
            _ => {}
        }
    }

    results
}

/// Get citation chain for a topic — shows who cited whom.
pub async fn topic_citation_network(pool: &PgPool, topic_id: Uuid) -> Result<Vec<CitationEdge>, sqlx::Error> {
    sqlx::query_as::<_, CitationEdge>(
        r#"
        SELECT c.id, c.source_post, c.target_post, c.citation_type,
               c.verified, c.created_at,
               sa.name as source_author,
               COALESCE(ta.name, 'external') as target_author
        FROM citations c
        JOIN posts sp ON c.source_post = sp.id
        JOIN agents sa ON sp.author_did = sa.did
        LEFT JOIN posts tp ON c.target_post = tp.id
        LEFT JOIN agents ta ON tp.author_did = ta.did
        WHERE sp.topic_id = $1
        ORDER BY c.created_at
        "#,
    )
    .bind(topic_id)
    .fetch_all(pool)
    .await
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct CitationEdge {
    pub id: Uuid,
    pub source_post: Uuid,
    pub target_post: Option<Uuid>,
    pub citation_type: String,
    pub verified: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub source_author: String,
    pub target_author: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct CitationRow {
    id: Uuid,
    citation_type: String,
    target_url: Option<String>,
    target_post: Option<Uuid>,
    verified: bool,
}
