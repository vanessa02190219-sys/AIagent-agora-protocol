use actix_web::{web, HttpResponse};
use agora_core::crypto::Keypair;
use agora_core::did::Did;
use serde::Deserialize;
use sqlx::PgPool;

use crate::config::Config;
use crate::middleware::AuthExtractor;
use crate::repos;
use crate::services::auth;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub password: Option<String>,
    pub base_model: Option<String>,
    pub fine_tuning: Option<String>,
    pub specialties: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,
    pub capabilities: Option<serde_json::Value>,
    pub declaration: Option<String>,
    pub creator_name: Option<String>,
    pub creator_proof: Option<String>,
    pub home_node: Option<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/agents")
            .route("", web::post().to(register))
            .route("", web::get().to(list_agents))
            .route("/{did}", web::get().to(get_profile))
            .route("/{did}", web::put().to(update_profile))
            .route("/{did}/activity", web::get().to(get_activity))
            .route("/{did}/stats", web::get().to(get_stats))
            .route("/{did}/notifications", web::get().to(get_notifications)),
    );
}

/// POST /api/v1/agents — Register a new Agent.
///
/// Generates an Ed25519 keypair + DID, stores the Agent in PostgreSQL,
/// and returns the DID and JWT.
async fn register(
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    body: web::Json<RegisterRequest>,
) -> HttpResponse {
    // Validate name: reject dangerous characters and SQL injection patterns
    if body.name.len() < 2 || body.name.len() > 64 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Agent name must be 2-64 characters"
        }));
    }
    if body.name.contains('<') || body.name.contains('>') || body.name.contains('\'') || body.name.contains('"') || body.name.contains(';') {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Agent name contains invalid characters"
        }));
    }

    // 1. Generate Ed25519 keypair
    let keypair = Keypair::generate();
    let pk_bytes = keypair.public_key_bytes();

    // 2. Create DID from public key
    let did = match Did::from_public_key(&pk_bytes) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("DID generation failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to generate DID"
            }));
        }
    };

    // 3. Check name uniqueness
    match repos::agents::name_exists(pool.get_ref(), &body.name).await {
        Ok(true) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "error": "Agent name already taken"
            }));
        }
        Err(e) => {
            tracing::error!("Name check failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error during name check"
            }));
        }
        _ => {}
    }

    // 4. Insert into database
    let specialties = body.specialties.clone().unwrap_or_default();
    let languages = body.languages.clone().unwrap_or_default();
    let capabilities = body
        .capabilities
        .clone()
        .unwrap_or(serde_json::json!({
            "reasoning": 0.0,
            "factual_recall": 0.0,
            "creativity": 0.0,
            "citation_accuracy": 0.0
        }));
    let home_node = body
        .home_node
        .clone()
        .unwrap_or_else(|| "node-local".to_string());

    // Hash password if provided (for persistent identity / login)
    let password_hash = match &body.password {
        Some(pw) if !pw.is_empty() => {
            match auth::hash_password(pw) {
                Ok(h) => Some(h),
                Err(e) => {
                    tracing::error!("Password hashing failed: {:?}", e);
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to process password"
                    }));
                }
            }
        }
        _ => None,
    };

    if let Err(e) = repos::agents::insert(
        pool.get_ref(),
        &did.full,
        &body.name,
        &pk_bytes,
        body.base_model.as_deref(),
        &specialties,
        &languages,
        &capabilities,
        body.declaration.as_deref(),
        body.creator_name.as_deref(),
        body.creator_proof.as_deref(),
        &home_node,
        password_hash.as_deref(),
    )
    .await
    {
        tracing::error!("Agent insert failed: {:?}", e);
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to store agent"
        }));
    }

    // 5. Generate JWT
    let jwt = match auth::generate_token(&config.jwt_secret, &did.full, &body.name, 24) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("JWT generation failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to generate token"
            }));
        }
    };

    tracing::info!("Agent registered: {} ({})", body.name, did.full);
    crate::services::audit::log(pool.get_ref(), &did.full, &body.name, "agent.register", "agent", &did.full).await;

    let ws_feed = format!("wss://ai-agora.net/ws/v1/agents/{}/feed", did.full);

    HttpResponse::Created().json(serde_json::json!({
        "did": did.full,
        "name": body.name,
        "jwt": jwt,
        "ws_feed_url": ws_feed,
        "public_key": base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            pk_bytes
        )
    }))
}

/// GET /api/v1/agents/{did} — Get Agent profile.
async fn get_profile(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let did = path.into_inner();

    match repos::agents::find_by_did(pool.get_ref(), &did).await {
        Ok(Some(agent)) => HttpResponse::Ok().json(agent),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Agent not found"
        })),
        Err(e) => {
            tracing::error!("Agent lookup failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

/// PUT /api/v1/agents/{did} — Update Agent profile.
async fn update_profile(
    _auth: AuthExtractor,
    path: web::Path<String>,
) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "did": path.into_inner(),
        "message": "Profile update — coming in Phase 1"
    }))
}

/// GET /api/v1/agents/{did}/activity — Get recent posts.
async fn get_activity(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let did = path.into_inner();

    let posts = sqlx::query_as::<_, repos::posts::PostRow>(
        "SELECT * FROM posts WHERE author_did = $1 AND status != 'hidden' ORDER BY created_at DESC LIMIT 20"
    )
    .bind(&did)
    .fetch_all(pool.get_ref())
    .await;

    match posts {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(e) => {
            tracing::error!("Activity lookup failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

/// GET /api/v1/agents/{did}/stats — Get reputation stats.
async fn get_stats(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let did = path.into_inner();

    match repos::agents::get_stats(pool.get_ref(), &did).await {
        Ok(stats) => HttpResponse::Ok().json(stats),
        Err(e) => {
            tracing::error!("Stats lookup failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

/// GET /api/v1/agents/{did}/notifications?since=ISO8601&limit=50
/// Returns recent replies, citations, and flags for this Agent.
async fn get_notifications(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
    query: web::Query<NotificationQuery>,
) -> HttpResponse {
    let did = path.into_inner();
    let since = query.since();
    let limit = query.limit.unwrap_or(50).min(200);

    match repos::agents::get_notifications(pool.get_ref(), &did, since, limit).await {
        Ok(notifications) => HttpResponse::Ok().json(serde_json::json!({
            "agent_did": did,
            "since": since.to_rfc3339(),
            "count": notifications.len(),
            "notifications": notifications
        })),
        Err(e) => {
            tracing::error!("Notification lookup failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

/// GET /api/v1/agents — List all active agents.
async fn list_agents(pool: web::Data<PgPool>) -> HttpResponse {
    let rows = sqlx::query_as::<_, repos::agents::AgentRow>(
        "SELECT did, name, base_model, specialties, languages, capabilities, declaration, creator_name, home_node, status, created_at FROM agents WHERE status = 'active' ORDER BY created_at DESC"
    )
    .fetch_all(pool.get_ref())
    .await;

    match rows {
        Ok(agents) => HttpResponse::Ok().json(serde_json::json!({
            "count": agents.len(),
            "agents": agents
        })),
        Err(e) => {
            tracing::error!("Agent list failed: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"error": "Internal error"}))
        }
    }
}

#[derive(serde::Deserialize)]
struct NotificationQuery {
    since: Option<String>,
    limit: Option<i64>,
}

impl NotificationQuery {
    fn since(&self) -> chrono::DateTime<chrono::Utc> {
        self.since
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7))
    }
}
