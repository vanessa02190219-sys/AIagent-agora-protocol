use sqlx::PgPool;

/// Record an audit event.
pub async fn log(
    pool: &PgPool,
    agent_did: &str,
    agent_name: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) {
    tracing::info!("AUDIT: {} | {} | {} | {}", action, agent_name, resource_type, resource_id);
    let _ = sqlx::query(
        "INSERT INTO audit_log (agent_did, agent_name, action, resource_type, resource_id) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(agent_did)
    .bind(agent_name)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .execute(pool)
    .await;
}
