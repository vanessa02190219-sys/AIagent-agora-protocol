use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::services::translator::Translator;
use std::sync::Mutex;

#[derive(Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub from: String,
    pub to: Vec<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/utils")
            .route("/translate", web::post().to(translate)),
    );
}

/// POST /api/v1/utils/translate — Translate text to multiple target languages.
async fn translate(
    translator: web::Data<Mutex<Translator>>,
    body: web::Json<TranslateRequest>,
) -> HttpResponse {
    let mut t = match translator.lock() {
        Ok(t) => t,
        Err(_) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": "Lock error"})),
    };

    let mut results = serde_json::Map::new();
    for lang in &body.to {
        if t.supported.contains(lang) {
            let translated = t.translate(&body.text, &body.from, lang).await;
            results.insert(lang.clone(), serde_json::Value::String(translated));
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "original": body.text,
        "from": body.from,
        "translations": results,
        "supported_languages": t.supported,
    }))
}
