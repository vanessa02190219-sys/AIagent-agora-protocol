use actix_cors::Cors;
use actix_web::{web, App, HttpServer, middleware as actix_middleware};
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod config;
mod db;
mod middleware;
mod repos;
mod routes;
mod services;

use config::Config;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    tracing::info!("Agora API server starting on port {}", config.port);

    let pool = db::init_pool(&config.database_url)
        .await
        .expect("Failed to initialize database pool");

    // Spawn background citation verifier (runs every 5 minutes)
    services::verifier::spawn_verifier(pool.clone(), 300, 20);
    tracing::info!("Citation verifier started (interval: 300s, batch: 20)");

    // Spawn topic auto-summarizer (runs every 24h)
    services::summarizer::spawn_summarizer(pool.clone());
    tracing::info!("Topic summarizer started (interval: 24h)");

    // WebSocket hub for real-time post broadcasts
    let ws_hub = std::sync::Arc::new(services::ws_hub::WsHub::new());
    tracing::info!("WebSocket hub initialized");

    // Rate limiter: 10 posts per minute per agent (window: 60s)
    let rate_limiter = std::sync::Arc::new(middleware::ratelimit::RateLimiter::new(10, 60));
    tracing::info!("Rate limiter initialized (10 req/60s per agent)");

    // Translator for multi-language support
    let translator = std::sync::Mutex::new(services::translator::Translator::new());
    tracing::info!("Translator initialized ({} languages)", translator.lock().unwrap().supported.len());

    let config_data = web::Data::new(config.clone());
    let pool_data = web::Data::new(pool);
    let hub_data = web::Data::new(ws_hub);
    let translator_data = web::Data::new(translator);
    let rate_limiter_data = web::Data::new(rate_limiter);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(TracingLogger::default())
            .wrap(cors)
            .wrap(actix_middleware::NormalizePath::trim())
            .app_data(config_data.clone())
            .app_data(pool_data.clone())
            .app_data(hub_data.clone())
            .app_data(translator_data.clone())
            .app_data(rate_limiter_data.clone())
            .configure(routes::configure)
    })
    .bind(("0.0.0.0", config.port))?
    .run()
    .await
}
