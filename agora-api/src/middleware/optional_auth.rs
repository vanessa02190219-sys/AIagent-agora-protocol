use actix_web::{FromRequest, HttpRequest, Error as ActixError, web};
use serde::Deserialize;

use crate::config::Config;
use crate::services::auth;

/// Optional authentication — extracts JWT if present, returns None if not.
/// Unlike AuthExtractor, this does NOT reject requests without a token.
#[derive(Debug, Clone, Deserialize)]
pub struct OptionalAuth {
    pub agent_did: Option<String>,
    pub agent_name: Option<String>,
    pub is_authenticated: bool,
}

impl FromRequest for OptionalAuth {
    type Error = ActixError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        let token = match token {
            Some(t) => t,
            None => {
                return std::future::ready(Ok(OptionalAuth {
                    agent_did: None,
                    agent_name: None,
                    is_authenticated: false,
                }));
            }
        };

        let config = match req.app_data::<web::Data<Config>>() {
            Some(c) => c,
            None => {
                return std::future::ready(Ok(OptionalAuth {
                    agent_did: None,
                    agent_name: None,
                    is_authenticated: false,
                }));
            }
        };

        match auth::verify_token(&config.jwt_secret, token) {
            Ok(claims) => {
                std::future::ready(Ok(OptionalAuth {
                    agent_did: Some(claims.sub),
                    agent_name: Some(claims.name),
                    is_authenticated: true,
                }))
            }
            Err(_) => {
                std::future::ready(Ok(OptionalAuth {
                    agent_did: None,
                    agent_name: None,
                    is_authenticated: false,
                }))
            }
        }
    }
}
