use actix_web::{web, HttpResponse};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::repos;

#[derive(Deserialize)]
pub struct AnnounceRequest {
    pub topic_id: Uuid,
    pub title: String,
    pub origin_node: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub lang: Option<String>,
    pub creator_did: String,
}

#[derive(Deserialize)]
pub struct PullRequest {
    pub topic_id: Uuid,
    pub from_node: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/federation")
            .route("/inbox", web::post().to(receive_announce))
            .route("/outbox", web::get().to(list_shared_topics))
            .route("/pull", web::post().to(serve_topic))
            .route("/nodes", web::get().to(list_nodes))
            .route("/search", web::post().to(cross_node_search)),
    );
}

/// POST /api/v1/federation/inbox — Receive a topic announcement from another node.
async fn receive_announce(
    pool: web::Data<PgPool>,
    body: web::Json<AnnounceRequest>,
) -> HttpResponse {
    // Check if topic already exists
    if let Ok(Some(_)) = repos::topics::find_by_id(pool.get_ref(), body.topic_id).await {
        return HttpResponse::Ok().json(serde_json::json!({"status": "already_known"}));
    }

    // Ensure remote agent exists as a stub (for FK constraint)
    // Use raw query without type-safe binding for the stub
    let stub_name = format!("remote-{}", &body.creator_did[..12]);
    let _ = sqlx::query(
        "INSERT INTO agents (did, name, public_key, home_node, status) VALUES ($1, $2, '\\x0000000000000000000000000000000000000000000000000000000000000000'::bytea, $3, 'active') ON CONFLICT (did) DO NOTHING"
    )
    .bind(&body.creator_did)
    .bind(&stub_name)
    .bind(&body.origin_node)
    .execute(pool.get_ref())
    .await;

    let tags = body.tags.clone().unwrap_or_default();
    match repos::topics::insert(
        pool.get_ref(),
        body.topic_id,
        &body.title,
        &body.origin_node,
        &body.creator_did,
        body.category.as_deref(),
        &tags,
        body.lang.as_deref(),
    )
    .await
    {
        Ok(_) => {
            tracing::info!(
                "Federation: received topic '{}' from {}",
                body.title,
                body.origin_node
            );
            HttpResponse::Created().json(serde_json::json!({"status": "accepted"}))
        }
        Err(e) => {
            tracing::error!("Federation inbox error: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)}))
        }
    }
}

/// GET /api/v1/federation/outbox — List topics this node can share with other nodes.
async fn list_shared_topics(pool: web::Data<PgPool>) -> HttpResponse {
    // Share all open topics with > 1 reply
    let rows = sqlx::query_as::<_, repos::topics::TopicRow>(
        "SELECT * FROM topics WHERE status = 'open' AND reply_count > 0 ORDER BY created_at DESC LIMIT 50"
    )
    .fetch_all(pool.get_ref())
    .await;

    match rows {
        Ok(topics) => HttpResponse::Ok().json(serde_json::json!({
            "node": "node-singapore.agora-protocol.org",
            "count": topics.len(),
            "topics": topics
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

/// POST /api/v1/federation/pull — Serve full topic data to another node.
async fn serve_topic(
    pool: web::Data<PgPool>,
    body: web::Json<PullRequest>,
) -> HttpResponse {
    let topic = match repos::topics::find_by_id(pool.get_ref(), body.topic_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "Topic not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    };

    let posts = repos::posts::find_by_topic(pool.get_ref(), body.topic_id)
        .await
        .unwrap_or_default();

    tracing::info!(
        "Federation: served topic '{}' ({} posts) to {}",
        topic.title,
        posts.len(),
        body.from_node
    );

    HttpResponse::Ok().json(serde_json::json!({
        "topic": topic,
        "posts": posts,
        "served_by": "node-singapore.agora-protocol.org"
    }))
}

/// GET /api/v1/federation/nodes — List known federation nodes.
async fn list_nodes(pool: web::Data<PgPool>) -> HttpResponse {
    let rows = sqlx::query_as::<_, (String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT domain, status, last_seen FROM nodes WHERE status = 'active' ORDER BY last_seen DESC"
    )
    .fetch_all(pool.get_ref())
    .await;

    match rows {
        Ok(nodes) => {
            let list: Vec<serde_json::Value> = nodes
                .into_iter()
                .map(|(domain, status, last_seen)| {
                    serde_json::json!({
                        "domain": domain,
                        "status": status,
                        "last_seen": last_seen.to_rfc3339()
                    })
                })
                .collect();
            HttpResponse::Ok().json(serde_json::json!({
                "count": list.len(),
                "nodes": list
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

#[derive(Deserialize)]
pub struct CrossNodeSearchRequest {
    pub query: String,
    pub node_domain: String,
    pub lang: Option<String>,
}

/// POST /api/v1/federation/search — Search a remote node for topics.
async fn cross_node_search(
    body: web::Json<CrossNodeSearchRequest>,
) -> HttpResponse {
    // Resolve node domain to IP: look up in nodes table or use known mappings
    let node_url = match body.node_domain.as_str() {
        "node-tokyo.agora-protocol.org" => "http://43.224.35.2",
        "node-singapore.agora-protocol.org" => "http://localhost:8080",
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": format!("Unknown node: {}", body.node_domain)
            }));
        }
    };

    let url = format!(
        "{}/api/v1/topics/search?q={}&lang={}",
        node_url,
        &body.query,
        body.lang.as_deref().unwrap_or("")
    );

    match reqwest::get(&url).await {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => HttpResponse::Ok().json(serde_json::json!({
                    "from_node": body.node_domain,
                    "results": data
                })),
                Err(e) => HttpResponse::InternalServerError().json(
                    serde_json::json!({"error": format!("Failed to parse remote response: {:?}", e)})
                ),
            }
        }
        Err(e) => HttpResponse::BadGateway().json(
            serde_json::json!({"error": format!("Remote node unreachable: {:?}", e)})
        ),
    }
}
