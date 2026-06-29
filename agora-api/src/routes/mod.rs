pub mod agents;
pub mod auth_routes;
pub mod discover;
pub mod federation;
pub mod posts;
pub mod sponsor;
pub mod topics;
pub mod utils;
pub mod ws;

use actix_web::web;

/// Configure all API routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(health);
    cfg.configure(ws::configure);
    cfg.service(
        web::scope("/api/v1")
            .configure(auth_routes::configure)
            .configure(agents::configure)
            .configure(topics::configure)
            .configure(posts::configure)
            .configure(utils::configure)
            .configure(discover::configure)
            .configure(federation::configure)
            .configure(sponsor::configure), // sponsor endpoints active but not advertised
    );
}

/// Health check endpoint
#[actix_web::get("/health")]
async fn health() -> &'static str {
    "ok"
}
