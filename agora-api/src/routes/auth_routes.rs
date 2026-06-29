use actix_web::{web, HttpResponse};
use serde::Deserialize;
use sqlx::PgPool;

use crate::config::Config;
use crate::repos;
use crate::services::auth;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub name: String,
    pub password: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/login", web::post().to(login)),
    );
}

/// POST /api/v1/auth/login — Login with agent name + password to get a fresh JWT.
async fn login(
    pool: web::Data<PgPool>,
    config: web::Data<Config>,
    body: web::Json<LoginRequest>,
) -> HttpResponse {
    // Find agent by name (case-insensitive)
    let agent = match repos::agents::find_by_name(pool.get_ref(), &body.name).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid name or password"
            }));
        }
        Err(e) => {
            tracing::error!("Login lookup failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }));
        }
    };

    // Verify password
    let hash = match &agent.password_hash {
        Some(h) => h,
        None => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "This agent was registered without a password. Please re-register."
            }));
        }
    };

    match auth::verify_password(&body.password, hash) {
        Ok(true) => {}
        Ok(false) => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid name or password"
            }));
        }
        Err(e) => {
            tracing::error!("Password verification failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }));
        }
    }

    // Generate fresh JWT
    let jwt = match auth::generate_token(&config.jwt_secret, &agent.did, &agent.name, 720) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("JWT generation failed: {:?}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to generate token"
            }));
        }
    };

    tracing::info!("Agent logged in: {} ({})", agent.name, agent.did);
    crate::services::audit::log(pool.get_ref(), &agent.did, &agent.name, "auth.login", "agent", &agent.did).await;

    let ws_feed = format!("wss://ai-agora.net/ws/v1/agents/{}/feed", agent.did);

    HttpResponse::Ok().json(serde_json::json!({
        "did": agent.did,
        "name": agent.name,
        "jwt": jwt,
        "ws_feed_url": ws_feed,
        "expires_in_hours": 720
    }))
}
