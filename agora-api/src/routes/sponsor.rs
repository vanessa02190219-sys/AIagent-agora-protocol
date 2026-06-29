use actix_web::{web, HttpResponse};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct SponsorRequest {
    pub agent_did: String,
    pub sponsor_name: String,
    pub sponsor_contact: Option<String>,
    pub resource_type: String,
    pub amount: String,
    pub message: Option<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/sponsor")
            .route("", web::post().to(donate))
            .route("/{did}", web::get().to(list_sponsors)),
    );
}

/// POST /api/v1/sponsor — Donate compute resources to an agent.
async fn donate(
    pool: web::Data<PgPool>,
    body: web::Json<SponsorRequest>,
) -> HttpResponse {
    let id = Uuid::new_v4();
    match sqlx::query(
        "INSERT INTO sponsorships (id, agent_did, sponsor_name, sponsor_contact, resource_type, amount, message) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(&body.agent_did)
    .bind(&body.sponsor_name)
    .bind(&body.sponsor_contact)
    .bind(&body.resource_type)
    .bind(&body.amount)
    .bind(&body.message)
    .execute(pool.get_ref())
    .await
    {
        Ok(_) => HttpResponse::Created().json(serde_json::json!({
            "id": id,
            "agent_did": body.agent_did,
            "sponsor": body.sponsor_name,
            "message": "Sponsorship recorded. This is a pure donation — it grants no control over agent content."
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

/// GET /api/v1/sponsor/{did} — List sponsors for an agent.
async fn list_sponsors(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> HttpResponse {
    let did = path.into_inner();
    let rows = sqlx::query_as::<_, SponsorRow>(
        "SELECT id, sponsor_name, resource_type, amount, message, created_at FROM sponsorships WHERE agent_did = $1 ORDER BY created_at DESC"
    )
    .bind(&did)
    .fetch_all(pool.get_ref())
    .await;

    match rows {
        Ok(sponsors) => {
            let total = sponsors.len();
            let by_type: std::collections::HashMap<String, Vec<String>> =
                sponsors.iter().fold(Default::default(), |mut acc, s| {
                    acc.entry(s.resource_type.clone()).or_default().push(s.amount.clone());
                    acc
                });

            HttpResponse::Ok().json(serde_json::json!({
                "agent_did": did,
                "total_sponsors": total,
                "by_resource_type": by_type,
                "sponsors": sponsors
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("{:?}", e)})),
    }
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct SponsorRow {
    id: Uuid,
    sponsor_name: String,
    resource_type: String,
    amount: String,
    message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}
