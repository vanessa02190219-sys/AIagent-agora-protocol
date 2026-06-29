use actix_web::{FromRequest, HttpRequest, Error as ActixError, web};
use serde::Deserialize;
use std::future::{ready, Ready};

use crate::config::Config;
use crate::services::auth;

/// Extracted Agent identity from a verified JWT.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthExtractor {
    pub agent_did: String,
    pub agent_name: String,
}

impl AuthExtractor {
    /// Check if this agent is a platform administrator.
    /// Admin is identified by cryptographically-immutable DID, not name.
    pub fn is_admin(&self) -> bool {
        // 丽娜's permanent DID — the only admin identity
        self.agent_did == "did:agora:z6MkmTQpuUG3v4vkLAyUCrrvtm2kmxxg8g59Uz3LNmuCa4yT"
    }
}

impl FromRequest for AuthExtractor {
    type Error = ActixError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        // Extract JWT from Authorization header
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        let token = match token {
            Some(t) => t,
            None => {
                return ready(Err(actix_web::error::ErrorUnauthorized(
                    "Missing Authorization: Bearer <token>",
                )));
            }
        };

        // Get config from app data
        let config = match req.app_data::<web::Data<Config>>() {
            Some(c) => c,
            None => {
                return ready(Err(actix_web::error::ErrorInternalServerError(
                    "Config not available",
                )));
            }
        };

        // Verify JWT
        match auth::verify_token(&config.jwt_secret, token) {
            Ok(claims) => {
                ready(Ok(AuthExtractor {
                    agent_did: claims.sub,
                    agent_name: claims.name,
                }))
            }
            Err(e) => {
                tracing::warn!("JWT verification failed: {:?}", e);
                ready(Err(actix_web::error::ErrorUnauthorized(
                    "Invalid or expired token",
                )))
            }
        }
    }
}
