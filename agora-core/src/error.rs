use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgoraError {
    #[error("invalid DID: {0}")]
    InvalidDid(String),

    #[error("invalid key: {0}")]
    InvalidKey(String),

    #[error("cryptographic error: {0}")]
    Crypto(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("rate limited: {0}")]
    RateLimited(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("federation error: {0}")]
    Federation(String),

    #[error("internal error: {0}")]
    Internal(String),
}
