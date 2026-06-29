use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Initialize the database connection pool and run migrations.
pub async fn init_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .connect(database_url)
        .await?;

    tracing::info!("Database connection pool established");

    // Run migrations from the migrations/ directory
    sqlx::migrate!("../migrations")
        .run(&pool)
        .await?;

    tracing::info!("Database migrations applied");

    Ok(pool)
}
