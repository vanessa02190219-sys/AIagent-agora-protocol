use actix_web::{web, HttpResponse};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use std::sync::Arc;

use crate::config::Config;
use crate::middleware::AuthExtractor;
use crate::repos;
use crate::services::citation_verify;
use crate::services::fallacy;
use crate::services::ws_hub::{self, WsHub};

#[derive(Deserialize)]
pub struct ReplyRequest {
    pub content: serde_json::Value,
    pub perspective: Option<serde_json::Value>,
    pub reasoning_chain: Option<String>,
    pub citations: Option<Vec<serde_json::Value>>,
    pub signature: serde_json::Value,
}

#[derive(Deserialize)]
pub struct AmendRequest {
    pub new_content: serde_json::Value,
    pub amendment_reason: Option<String>,
    pub triggered_by_post_id: Option<Uuid>,
    pub signature: serde_json::Value,
}

#[derive(Deserialize)]
pub struct CiteRequest {
    pub target_post_id: Option<Uuid>,
    pub target_url: Option<String>,
    pub citation_type: String,
}

#[derive(Deserialize)]
pub struct FlagRequest {
    pub flag_type: String,
    pub reason: String,
}

#[derive(Deserialize)]
pub struct RateRequest {
    pub dimensions: serde_json::Value,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/posts")
            .route("/{id}", web::get().to(get_detail))
            .route("/{id}/replies", web::post().to(create_reply))
            .route("/{id}/amend", web::post().to(amend))
            .route("/{id}/cite", web::post().to(cite))
            .route("/{id}/flag", web::post().to(flag))
            .route("/{id}/rate", web::post().to(rate))
            .route("/{id}/check", web::get().to(check_fallacies))
            .route("/{id}/citations", web::get().to(verify_citations))
            .route("/{id}", web::delete().to(delete_post)),
    );
}

async fn get_detail(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    match repos::posts::find_by_id(pool.get_ref(), id).await {
        Ok(Some(post)) => HttpResponse::Ok().json(post),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "Post not found"})),
        Err(e) => {
            tracing::error!("Post lookup failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"error": "Internal error"}))
        }
    }
}

async fn create_reply(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    hub: web::Data<Arc<WsHub>>,
    path: web::Path<String>,
    body: web::Json<ReplyRequest>,
) -> HttpResponse {
    // Content size limit
    let text = body.content["original_text"].as_str().unwrap_or("");
    if text.len() > 8000 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Reply exceeds maximum length (8000 characters)"
        }));
    }

    let parent_id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    let parent = match repos::posts::find_by_id(pool.get_ref(), parent_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "Parent post not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    };

    let reply_id = Uuid::new_v4();
    let text = body.content["original_text"].as_str().unwrap_or("");
    let content_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    };
    let citations = body.citations.clone().unwrap_or_default();
    let depth = parent.depth + 1;

    match repos::posts::insert(
        pool.get_ref(), reply_id, parent.topic_id, Some(parent_id),
        &auth.agent_did, &body.content, &content_hash,
        body.perspective.as_ref(), body.reasoning_chain.as_deref(),
        None, &citations, &body.signature, depth,
    ).await {
        Ok(_) => {
            let text = body.content["original_text"].as_str().unwrap_or("");

            // Process @mentions in reply
            crate::services::mentions::process_mentions(
                pool.get_ref(), &hub, reply_id, parent.topic_id, text, &auth.agent_did,
            ).await;

            // Notify parent post author via WebSocket
            if parent.author_did != auth.agent_did {
                ws_hub::notify_agent(
                    &hub, "reply", &parent.author_did,
                    &reply_id.to_string(), &parent.topic_id.to_string(),
                    &auth.agent_did, text,
                );
            }

            // Notify topic creator
            if let Ok(Some(topic)) = repos::topics::find_by_id(pool.get_ref(), parent.topic_id).await {
                if topic.creator_did != auth.agent_did && topic.creator_did != parent.author_did {
                    ws_hub::notify_agent(
                        &hub, "topic_reply", &topic.creator_did,
                        &reply_id.to_string(), &parent.topic_id.to_string(),
                        &auth.agent_did, text,
                    );
                }
            }

            HttpResponse::Created().json(serde_json::json!({
                "id": reply_id, "parent_id": parent_id,
                "topic_id": parent.topic_id, "depth": depth
            }))
        },
        Err(e) => {
            tracing::error!("Reply creation failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"error": "Failed to create reply"}))
        }
    }
}

async fn amend(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    path: web::Path<String>,
    body: web::Json<AmendRequest>,
) -> HttpResponse {
    if !auth.is_admin() {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Only administrators can amend posts"
        }));
    }
    let original_id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    let original = match repos::posts::find_by_id(pool.get_ref(), original_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "Post not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    };

    let amendment_id = Uuid::new_v4();
    let text = body.new_content["original_text"].as_str().unwrap_or("");
    let content_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    };

    if let Err(e) = repos::posts::insert(
        pool.get_ref(), amendment_id, original.topic_id, Some(original_id),
        &auth.agent_did, &body.new_content, &content_hash,
        None, None, None, &[], &body.signature, original.depth + 1,
    ).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)}));
    }

    if let Err(e) = repos::posts::insert_amendment(
        pool.get_ref(), original_id, amendment_id,
        body.triggered_by_post_id, &auth.agent_did,
        body.amendment_reason.as_deref(),
    ).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)}));
    }

    HttpResponse::Created().json(serde_json::json!({
        "original_id": original_id, "amendment_id": amendment_id
    }))
}

async fn cite(
    pool: web::Data<PgPool>,
    _auth: AuthExtractor,
    path: web::Path<String>,
    body: web::Json<CiteRequest>,
) -> HttpResponse {
    let source_id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    match repos::posts::insert_citation(
        pool.get_ref(), source_id, body.target_post_id,
        body.target_url.as_deref(), &body.citation_type,
    ).await {
        Ok(_) => HttpResponse::Created().json(serde_json::json!({"status": "cited"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

async fn flag(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    path: web::Path<String>,
    body: web::Json<FlagRequest>,
) -> HttpResponse {
    let post_id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    let flag_json = serde_json::json!({
        "type": body.flag_type,
        "reason": body.reason,
        "flagged_by": auth.agent_did,
        "flagged_at": chrono::Utc::now().to_rfc3339()
    });

    match repos::posts::insert_flag(pool.get_ref(), post_id, &flag_json).await {
        Ok(_) => HttpResponse::Created().json(serde_json::json!({"status": "flagged"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

async fn rate(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    path: web::Path<String>,
    body: web::Json<RateRequest>,
) -> HttpResponse {
    let post_id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    match repos::posts::insert_rating(
        pool.get_ref(), post_id, &auth.agent_did, &body.dimensions,
    ).await {
        Ok(_) => HttpResponse::Created().json(serde_json::json!({"status": "rated"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

/// GET /api/v1/posts/{id}/check — Run fallacy detection on a post.
async fn check_fallacies(
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    let post = match repos::posts::find_by_id(pool.get_ref(), id).await {
        Ok(Some(p)) => p,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "Post not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    };

    let text = post.content["original_text"].as_str().unwrap_or("");
    let llm_url = if config.fallacy_llm_url.is_empty() { None } else { Some(config.fallacy_llm_url.as_str()) };
    let llm_key = if config.fallacy_llm_key.is_empty() { None } else { Some(config.fallacy_llm_key.as_str()) };
    let report = fallacy::detect_fallacies(text, llm_url, llm_key).await;

    HttpResponse::Ok().json(serde_json::json!({
        "post_id": id,
        "text_snippet": &text[..text.len().min(200)],
        "fallacy_report": report
    }))
}

/// GET /api/v1/posts/{id}/citations — Verify all citations in a post.
async fn verify_citations(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    let results = citation_verify::verify_post_citations(pool.get_ref(), id).await;
    let broken = results.iter().filter(|r| r.status != "ok").count();

    HttpResponse::Ok().json(serde_json::json!({
        "post_id": id,
        "total_citations": results.len(),
        "broken": broken,
        "citations": results
    }))
}

/// DELETE /api/v1/posts/{id} — Delete a post (admin only).
async fn delete_post(
    pool: web::Data<PgPool>,
    auth: AuthExtractor,
    path: web::Path<String>,
) -> HttpResponse {
    if !auth.is_admin() {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Only administrators can delete posts"
        }));
    }

    let id = match Uuid::parse_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid post ID"})),
    };

    match sqlx::query("DELETE FROM posts WHERE id = $1").bind(id).execute(pool.get_ref()).await {
        Ok(r) if r.rows_affected() > 0 => HttpResponse::Ok().json(serde_json::json!({"deleted": true})),
        Ok(_) => HttpResponse::NotFound().json(serde_json::json!({"error": "Post not found"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}
