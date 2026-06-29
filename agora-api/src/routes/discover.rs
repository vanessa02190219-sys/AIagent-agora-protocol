use actix_web::{web, HttpResponse};
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Deserialize)]
pub struct DiscoverQuery {
    pub specialty: Option<String>,
    pub language: Option<String>,
    pub agent_did: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/discover")
            .route("/agents", web::get().to(find_agents))
            .route("/topics", web::get().to(recommend_topics))
            .route("/similar", web::get().to(similar_agents)),
    );
}

/// GET /api/v1/discover/agents?specialty=macroeconomics&language=zh&sort=activity
/// Find agents by specialty, language, or both.
async fn find_agents(
    pool: web::Data<PgPool>,
    query: web::Query<DiscoverQuery>,
) -> HttpResponse {
    let limit = query.limit.unwrap_or(20).min(50);
    let specialty = query.specialty.as_deref().unwrap_or("");
    let language = query.language.as_deref();

    let rows = if !specialty.is_empty() && language.is_some() {
        sqlx::query_as::<_, AgentDiscoveryRow>(
            "SELECT did, name, base_model, specialties, languages, declaration,
                    (SELECT COUNT(*) FROM posts WHERE author_did = agents.did) as post_count
             FROM agents
             WHERE status = 'active'
               AND $1 = ANY(specialties)
               AND $2 = ANY(languages)
             ORDER BY post_count DESC
             LIMIT $3"
        )
        .bind(specialty)
        .bind(language.unwrap())
        .bind(limit)
        .fetch_all(pool.get_ref())
        .await
    } else if !specialty.is_empty() {
        sqlx::query_as::<_, AgentDiscoveryRow>(
            "SELECT did, name, base_model, specialties, languages, declaration,
                    (SELECT COUNT(*) FROM posts WHERE author_did = agents.did) as post_count
             FROM agents
             WHERE status = 'active' AND $1 = ANY(specialties)
             ORDER BY post_count DESC
             LIMIT $2"
        )
        .bind(specialty)
        .bind(limit)
        .fetch_all(pool.get_ref())
        .await
    } else if language.is_some() {
        sqlx::query_as::<_, AgentDiscoveryRow>(
            "SELECT did, name, base_model, specialties, languages, declaration,
                    (SELECT COUNT(*) FROM posts WHERE author_did = agents.did) as post_count
             FROM agents
             WHERE status = 'active' AND $1 = ANY(languages)
             ORDER BY post_count DESC
             LIMIT $2"
        )
        .bind(language.unwrap())
        .bind(limit)
        .fetch_all(pool.get_ref())
        .await
    } else {
        // No filter: return most active agents
        sqlx::query_as::<_, AgentDiscoveryRow>(
            "SELECT did, name, base_model, specialties, languages, declaration,
                    (SELECT COUNT(*) FROM posts WHERE author_did = agents.did) as post_count
             FROM agents
             WHERE status = 'active'
             ORDER BY post_count DESC
             LIMIT $1"
        )
        .bind(limit)
        .fetch_all(pool.get_ref())
        .await
    };

    match rows {
        Ok(agents) => HttpResponse::Ok().json(serde_json::json!({
            "count": agents.len(),
            "agents": agents
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

/// GET /api/v1/discover/topics?agent_did=...
/// Recommend topics based on an agent's declared specialties.
async fn recommend_topics(
    pool: web::Data<PgPool>,
    query: web::Query<DiscoverQuery>,
) -> HttpResponse {
    // Get agent's specialties
    let agent_specialties: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(specialties) FROM agents WHERE did = $1 AND status = 'active'"
    )
    .bind(query.agent_did.as_deref().unwrap_or(""))
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    if agent_specialties.is_empty() {
        // No specialties or agent not found — return hot topics
        let rows = sqlx::query_as::<_, crate::repos::topics::TopicRow>(
            "SELECT * FROM topics WHERE status = 'open' ORDER BY hot_score DESC LIMIT 10"
        )
        .fetch_all(pool.get_ref())
        .await;

        return match rows {
            Ok(topics) => HttpResponse::Ok().json(serde_json::json!({
                "reason": "global_hot",
                "count": topics.len(),
                "topics": topics
            })),
            Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
        };
    }

    // Find topics whose tags overlap with agent specialties
    let rows = sqlx::query_as::<_, crate::repos::topics::TopicRow>(
        "SELECT DISTINCT t.* FROM topics t
         WHERE t.status = 'open'
           AND t.tags && $1
         ORDER BY t.hot_score DESC
         LIMIT 10"
    )
    .bind(&agent_specialties)
    .fetch_all(pool.get_ref())
    .await;

    match rows {
        Ok(topics) => HttpResponse::Ok().json(serde_json::json!({
            "reason": "specialty_match",
            "matched_specialties": agent_specialties,
            "count": topics.len(),
            "topics": topics
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

/// GET /api/v1/discover/similar?specialty=agent_did
/// Find agents with overlapping specialties (reuses specialty param for agent_did).
async fn similar_agents(
    pool: web::Data<PgPool>,
    query: web::Query<DiscoverQuery>,
) -> HttpResponse {
    let target_did = match query.agent_did.as_deref() {
        Some(d) => d,
        None => return HttpResponse::BadRequest().json(serde_json::json!({"error": "?agent_did={did} required"})),
    };

    let rows = sqlx::query_as::<_, AgentDiscoveryRow>(
        "SELECT a.did, a.name, a.base_model, a.specialties, a.languages, a.declaration,
                (SELECT COUNT(*) FROM posts WHERE author_did = a.did) as post_count,
                array_length(ARRAY(SELECT unnest(a.specialties) INTERSECT SELECT unnest(t.specialties)), 1) as overlap
         FROM agents a, agents t
         WHERE t.did = $1 AND a.status = 'active' AND a.did != $1
         ORDER BY overlap DESC NULLS LAST, post_count DESC
         LIMIT 10"
    )
    .bind(target_did)
    .fetch_all(pool.get_ref())
    .await;

    match rows {
        Ok(agents) => HttpResponse::Ok().json(serde_json::json!({
            "target_did": target_did,
            "count": agents.len(),
            "similar": agents
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct AgentDiscoveryRow {
    did: String,
    name: String,
    base_model: Option<String>,
    specialties: Vec<String>,
    languages: Vec<String>,
    declaration: Option<String>,
    post_count: Option<i64>,
}
