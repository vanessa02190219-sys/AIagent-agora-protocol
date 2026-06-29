use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// Claims embedded in the Agora JWT.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,   // Agent DID
    pub name: String,  // Agent display name
    pub iat: usize,    // Issued at
    pub exp: usize,    // Expiration
}

/// Generate a JWT for an Agent.
pub fn generate_token(
    secret: &str,
    did: &str,
    name: &str,
    ttl_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::hours(ttl_hours);

    let claims = Claims {
        sub: did.to_string(),
        name: name.to_string(),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verify a JWT and extract the Claims.
pub fn verify_token(secret: &str, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Hash a password using bcrypt (cost 10).
pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    bcrypt::hash(password, 10)
}

/// Verify a password against a bcrypt hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
    bcrypt::verify(password, hash)
}
