use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_ws;
use sqlx::PgPool;
use std::sync::Arc;
use futures_util::StreamExt;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::repos;
use crate::services::auth;
use crate::services::ws_hub::{AgentNotification, WsHub};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/ws/v1")
            .route("/feed", web::get().to(feed_handler))
            .route("/agents/{did}/feed", web::get().to(agent_feed_handler)),
    );
}

async fn feed_handler(
    req: HttpRequest,
    stream: web::Payload,
    hub: web::Data<Arc<WsHub>>,
) -> Result<HttpResponse, Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let mut rx = hub.subscribe_posts();

    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                msg = msg_stream.next() => {
                    match msg {
                        Some(Ok(actix_ws::Message::Ping(bytes))) => { let _ = session.pong(&bytes).await; }
                        Some(Ok(actix_ws::Message::Close(_))) => break,
                        None => break,
                        _ => {}
                    }
                }
                event = rx.recv() => {
                    match event {
                        Ok(e) => {
                            if session.text(serde_json::to_string(&e).unwrap_or_default()).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            }
        }
    });

    Ok(response)
}

async fn agent_feed_handler(
    req: HttpRequest,
    stream: web::Payload,
    pool: web::Data<PgPool>,
    hub: web::Data<Arc<WsHub>>,
    config: web::Data<Config>,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let agent_did = path.into_inner();

    // --- Authentication: require JWT in query param, DID must match ---
    let query = web::Query::<SinceQuery>::from_query(req.query_string()).ok();
    let token = query.as_ref().and_then(|q| q.token.as_deref());

    match token {
        Some(t) => {
            match auth::verify_token(&config.jwt_secret, t) {
                Ok(claims) if claims.sub == agent_did => {
                    // Authenticated — DID matches JWT
                }
                Ok(_) => {
                    return Err(actix_web::error::ErrorUnauthorized("DID does not match JWT"));
                }
                Err(_) => {
                    return Err(actix_web::error::ErrorUnauthorized("Invalid or expired token"));
                }
            }
        }
        None => {
            return Err(actix_web::error::ErrorUnauthorized("Missing ?token={jwt} query parameter"));
        }
    }

    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    let since = query
        .and_then(|q| q.since_dt())
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(3));

    // --- Phase 1: Push historical (missed) notifications ---
    match repos::agents::get_notifications(pool.get_ref(), &agent_did, since, 30).await {
        Ok(notifications) => {
            for n in &notifications {
                let event = AgentNotification {
                    event: n.kind.clone(),
                    target_did: agent_did.clone(),
                    post_id: n.ref_id.clone(),
                    topic_id: n.topic_id.clone().unwrap_or_default(),
                    actor_did: n.actor_did.clone().unwrap_or_default(),
                    snippet: n.snippet.clone().unwrap_or_default(),
                    created_at: n.created_at.to_rfc3339(),
                };
                let json = serde_json::to_string(&event).unwrap_or_default();
                if session.text(json).await.is_err() {
                    return Ok(response);
                }
            }
            if !notifications.is_empty() {
                tracing::info!("WS: pushed {} historical notifications to {}", notifications.len(), &agent_did[..40]);
            }
        }
        Err(e) => {
            tracing::warn!("WS: failed to fetch historical notifications: {:?}", e);
        }
    }

    // --- Phase 2: Real-time broadcast ---
    let mut rx = hub.subscribe_notifications();

    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                msg = msg_stream.next() => {
                    match msg {
                        Some(Ok(actix_ws::Message::Ping(bytes))) => { let _ = session.pong(&bytes).await; }
                        Some(Ok(actix_ws::Message::Close(_))) => break,
                        None => break,
                        _ => {}
                    }
                }
                event = rx.recv() => {
                    match event {
                        Ok(e) if e.target_did == agent_did => {
                            if session.text(serde_json::to_string(&e).unwrap_or_default()).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        _ => {}
                    }
                }
            }
        }
    });

    Ok(response)
}

#[derive(serde::Deserialize)]
struct SinceQuery {
    since: Option<String>,
    token: Option<String>,
}

impl SinceQuery {
    fn since_dt(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.since
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
    }
}
