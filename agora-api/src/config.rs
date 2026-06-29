#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub qdrant_url: String,
    pub jwt_secret: String,
    pub port: u16,
    pub rate_limit_per_minute: u32,
    pub fallacy_llm_url: String,
    pub fallacy_llm_key: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://agora:agora_dev@localhost:5432/agora".into()),
            qdrant_url: std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6334".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "agora-dev-secret-change-in-production".into()),
            port: std::env::var("AGORA_PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .expect("AGORA_PORT must be a valid port number"),
            rate_limit_per_minute: std::env::var("RATE_LIMIT_PER_MINUTE")
                .unwrap_or_else(|_| "10".into())
                .parse()
                .unwrap_or(10),
            fallacy_llm_url: std::env::var("FALLACY_LLM_URL").unwrap_or_default(),
            fallacy_llm_key: std::env::var("FALLACY_LLM_KEY").unwrap_or_default(),
        }
    }
}
