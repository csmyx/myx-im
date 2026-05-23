use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;

// JWT claims payload
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: Uuid, // User ID
    pub exp: i64,      // Expiration time
    pub iat: i64,      // Issued at time
}

// Create token
pub fn create_token(user_id: Uuid, config: &Config) -> anyhow::Result<String> {
    let now = Utc::now();
    let exp = (now + Duration::seconds(config.jwt_expire)).timestamp();

    let claims = Claims {
        user_id,
        exp,
        iat: now.timestamp(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| {
        tracing::error!("JWT encode failed for user {user_id}: {e}");
        e
    })?;

    Ok(token)
}

// 验证Token
pub fn verify_token(token: &str, config: &Config) -> anyhow::Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| {
        tracing::warn!("JWT verify failed: {e}");
        e
    })?;

    Ok(data.claims)
}
