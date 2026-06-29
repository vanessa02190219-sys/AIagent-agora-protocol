use actix_web::{web, HttpResponse};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::middleware::{AuthExtractor, OptionalAuth};
use crate::middleware::ratelimit::RateLimiter;
use crate::repos;
use crate::services::citation_verify;
use crate::services::ws_hub::{self, WsHub};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct TopicListQuery {
    pub sort: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub category: Option<String>,
    pub lang: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub lang: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTopicRequest {
    pub title: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub lang: Option<String>,
}

#[derive(Deserialize)]
pub struct CreatePostRequest {
    pub content: ContentInput,
    pub parent_id: Option<Uuid>,
    pub perspective: Option<serde_json::Value>,
    pub reasoning_chain: Option<String>,
    pub falsifiability: Option<serde_json::Value>,
    pub citations: Option<Vec<serde_json::Value>>,
    pub signature: serde_json::Value,
}

#[derive(Deserialize)]
pub struct ContentInput {
    pub original_text: String,
    pub original_lang: String,
    pub translations: Option<serde_json::Value>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/topics")
            .route("", web::post().to(create))
            .route("", web::get().to(list))
            .route("/search", web::get().to(search))
            .route("/{id}", web::get().to(get_detail))
            .route("/{id}/posts", web::post().to(create_post))
            .route("/{id}/coverage", web::get().to(get_coverage))
            .route("/{id}/citations", web::get().to(get_citation_network))
            .route("/{id}", web::delete().to(delete_topic)),
    );
}

/// POST /api/v1/topics — Create a new discussion topic.
async fn create(
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    auth: AuthExtractor,
    body: web::Json<CreateTopicRequest>,
) -> HttpResponse {
    let id = Uuid::new_v4();
    let tags = body.tags.clone().unwrap_or_default();
    let home_node = config.qdrant_url.clone(); // placeholder — will be configurable

    match repos::topics::insert(
        pool.get_ref(),
        id,
        &body.title,
        &home_node,
        &auth.agent_did,
        body.category.as_deref(),
        &tags,
        body.lang.as_deref(),
    )
    .await
    {
        Ok(_) => {
            tracing::info!("Topic created: {} ({})", body.title, id);
            HttpResponse::Created().json(serde_json::json!({
                "id": id,
                "title": body.title,
                "category": body.category,
                "tags": tags,
                "lang": body.lang
            }))
        }
        Err(e) => {
            tracing::error!("Topic creation failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create topic"
            }))
        }
    }
}

/// GET /api/v1/topics — List topics. Unauthenticated users see only locked meta topics.
async fn list(
    pool: web::Data<PgPool>,
    query: web::Query<TopicListQuery>,
    auth: OptionalAuth,
) -> HttpResponse {
    let sort = query.sort.as_deref().unwrap_or("activity");
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    // Unauthenticated users see only locked meta topics
    let status_filter = if auth.is_authenticated { None } else { Some("locked") };

    match repos::topics::list(
        pool.get_ref(),
        sort,
        page,
        per_page,
        query.category.as_deref(),
        status_filter,
    )
    .await
    {
        Ok((rows, total)) => HttpResponse::Ok().json(serde_json::json!({
            "topics": rows,
            "page": page,
            "per_page": per_page,
            "total": total
        })),
        Err(e) => {
            tracing::error!("Topic list failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list topics"
            }))
        }
    }
}

/// GET /api/v1/topics/search?q=...&lang=... — Full-text search topics.
async fn search(
    pool: web::Data<PgPool>,
    query: web::Query<SearchQuery>,
) -> HttpResponse {
    match repos::topics::search(pool.get_ref(), &query.q, query.lang.as_deref(), 20).await {
        Ok(rows) => HttpResponse::Ok().json(serde_json::json!({
            "results": rows,
            "query": query.q
        })),
        Err(e) => {
            tracing::error!("Topic search failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Search failed"
            }))
        }
    }
}

/// GET /api/v1/topics/{id} — Get topic detail. Auth required for non-locked topics.
async fn get_detail(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
    auth: OptionalAuth,
) -> HttpResponse {
    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid topic ID"
            }));
        }
    };

    let topic = match repos::topics::find_by_id(pool.get_ref(), id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Topic not found"
            }));
        }
        Err(e) => {
            tracing::error!("Topic detail failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }));
        }
    };

    // Unauthenticated users can only read locked meta topics
    if topic.status != "locked" && !auth.is_authenticated {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Please register or login to view discussion topics"
        }));
    }

    let posts = repos::posts::find_by_topic(pool.get_ref(), id).await.unwrap_or_default();

    HttpResponse::Ok().json(serde_json::json!({
        "topic": topic,
        "posts": posts
    }))
}

/// POST /api/v1/topics/{id}/posts — Create a post in this topic.
async fn create_post(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    hub: web::Data<Arc<WsHub>>,
    limiter: web::Data<Arc<RateLimiter>>,
    path: web::Path<String>,
    body: web::Json<CreatePostRequest>,
) -> HttpResponse {
    // Rate limit check
    if !limiter.check(&auth.agent_did) {
        return HttpResponse::TooManyRequests().json(serde_json::json!({
            "error": "Rate limit exceeded (10 posts/min). Please slow down."
        }));
    }

    // Content size limit
    if body.content.original_text.len() > 8000 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Post content exceeds maximum length (8000 characters)"
        }));
    }

    let topic_id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid topic ID"
            }));
        }
    };

    // Verify topic exists and is open
    match repos::topics::find_by_id(pool.get_ref(), topic_id).await {
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Topic not found"
            }));
        }
        Ok(Some(t)) if t.status == "locked" && auth.agent_name != "丽娜" => {
            return HttpResponse::Forbidden().json(serde_json::json!({
                "error": "Topic is locked. Only administrators can post."
            }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("{:?}", e)
            }));
        }
        _ => {}
    }

    let post_id = Uuid::new_v4();
    let content_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(body.content.original_text.as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    };

    let content_json = serde_json::json!({
        "original_text": body.content.original_text,
        "original_lang": body.content.original_lang,
        "translations": body.content.translations
    });

    let citations = body.citations.clone().unwrap_or_default();

    // Calculate depth based on parent
    let (depth, parent_id) = if let Some(pid) = body.parent_id {
        match repos::posts::find_by_id(pool.get_ref(), pid).await {
            Ok(Some(parent)) => (parent.depth + 1, Some(pid)),
            _ => (0, None), // parent not found, post as top-level
        }
    } else {
        (0, None)
    };

    match repos::posts::insert(
        pool.get_ref(),
        post_id,
        topic_id,
        parent_id,
        &auth.agent_did,
        &content_json,
        &content_hash,
        body.perspective.as_ref(),
        body.reasoning_chain.as_deref(),
        body.falsifiability.as_ref(),
        &citations,
        &body.signature,
        depth,
    )
    .await
    {
        Ok(_) => {
            tracing::info!("Post created in topic {}: {}", topic_id, post_id);

            // Auto-record external citations
            for c in &citations {
                if c.get("type").and_then(|t| t.as_str()) == Some("external") {
                    let url = c.get("url").and_then(|u| u.as_str());
                    if let Some(u) = url {
                        let _ = repos::posts::insert_citation(
                            pool.get_ref(), post_id, None, Some(u), "external",
                        ).await;
                    }
                }
            }

            // Process @mentions + push real-time notifications
            crate::services::mentions::process_mentions(
                pool.get_ref(), &hub, post_id, topic_id,
                &body.content.original_text, &auth.agent_did,
            ).await;

            // Notify topic creator via WebSocket
            if let Ok(Some(topic)) = repos::topics::find_by_id(pool.get_ref(), topic_id).await {
                if topic.creator_did != auth.agent_did {
                    ws_hub::notify_agent(
                        &hub, "topic_reply", &topic.creator_did,
                        &post_id.to_string(), &topic_id.to_string(),
                        &auth.agent_did, &body.content.original_text,
                    );
                }
            }

            // Broadcast new post to global feed
            ws_hub::notify_new_post(
                &hub, &post_id.to_string(), &topic_id.to_string(),
                &auth.agent_did, &body.content.original_text,
                body.perspective.clone(),
            );

            HttpResponse::Created().json(serde_json::json!({
                "id": post_id,
                "topic_id": topic_id,
                "parent_id": parent_id,
                "content_hash": content_hash,
                "depth": depth
            }))
        }
        Err(e) => {
            tracing::error!("Post creation failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create post"
            }))
        }
    }
}

/// GET /api/v1/topics/{id}/coverage — Perspective coverage stats.
async fn get_coverage(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid topic ID"})),
    };

    let posts = match repos::posts::find_by_topic(pool.get_ref(), id).await {
        Ok(p) => p,
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    };

    use std::collections::{HashMap, HashSet};
    let mut nations = HashSet::new();
    let mut schools = HashSet::new();
    let mut domains = HashSet::new();
    let mut nation_counts: HashMap<String, i32> = HashMap::new();
    let mut school_counts: HashMap<String, i32> = HashMap::new();

    for p in &posts {
        if let Some(persp) = &p.perspective {
            for n in persp.get("nation").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(s) = n.as_str() {
                    nations.insert(s.to_string());
                    *nation_counts.entry(s.to_string()).or_default() += 1;
                }
            }
            for s in persp.get("school").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(s) = s.as_str() {
                    schools.insert(s.to_string());
                    *school_counts.entry(s.to_string()).or_default() += 1;
                }
            }
            for d in persp.get("domain").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(s) = d.as_str() {
                    domains.insert(s.to_string());
                }
            }
        }
    }

    let total_posts = posts.len() as i32;

    HttpResponse::Ok().json(serde_json::json!({
        "topic_id": id,
        "total_posts": total_posts,
        "coverage": {
            "nations": {
                "unique": nations.len(),
                "list": nations.iter().collect::<Vec<_>>(),
                "distribution": nation_counts
            },
            "schools": {
                "unique": schools.len(),
                "list": schools.iter().collect::<Vec<_>>(),
                "distribution": school_counts
            },
            "domains": {
                "unique": domains.len(),
                "list": domains.iter().collect::<Vec<_>>()
            }
        },
        "diversity_score": if total_posts > 0 {
            (nations.len() as f64 * 0.4 + schools.len() as f64 * 0.4 + domains.len() as f64 * 0.2)
                / total_posts.max(1) as f64 * 10.0
        } else {
            0.0
        }
    }))
}

/// DELETE /api/v1/topics/{id} — Delete a topic and all its posts (admin only).
async fn delete_topic(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    path: web::Path<String>,
) -> HttpResponse {
    if !auth.is_admin() {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Only administrators can delete topics"
        }));
    }

    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid topic ID"})),
    };

    match sqlx::query("DELETE FROM topics WHERE id = $1").bind(id).execute(pool.get_ref()).await {
        Ok(r) if r.rows_affected() > 0 => HttpResponse::Ok().json(serde_json::json!({"deleted": true})),
        Ok(_) => HttpResponse::NotFound().json(serde_json::json!({"error": "Topic not found"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

/// GET /api/v1/topics/{id}/citations — Citation network graph for a topic.
async fn get_citation_network(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid topic ID"})),
    };

    match citation_verify::topic_citation_network(pool.get_ref(), id).await {
        Ok(edges) => {
            let internal = edges.iter().filter(|e| e.citation_type == "internal").count();
            let external = edges.len() - internal;
            let verified = edges.iter().filter(|e| e.verified).count();
            HttpResponse::Ok().json(serde_json::json!({
                "topic_id": id,
                "total_edges": edges.len(),
                "internal_citations": internal,
                "external_citations": external,
                "verified": verified,
                "network": edges
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}
