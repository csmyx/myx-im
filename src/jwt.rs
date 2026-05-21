use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;

// JWT 载荷
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: Uuid, // 用户ID
    pub exp: i64,      // 过期时间
    pub iat: i64,      // 签发时间
}

// 签发Token
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
    )?;

    Ok(token)
}

// 验证Token
pub fn verify_token(token: &str, config: &Config) -> anyhow::Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;

    Ok(data.claims)
}
