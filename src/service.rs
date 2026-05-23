use axum::{Json, http::StatusCode};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::dao;
use crate::jwt::create_token;
use crate::model::Res;

pub async fn register_user(
    pool: &PgPool,
    config: &Config,
    username: String,
    password: String,
) -> (StatusCode, Json<Res<String>>) {
    let Ok(password_hash) = bcrypt::hash(&password, bcrypt::DEFAULT_COST) else {
        tracing::error!("bcrypt hash failed for user {username}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Res::error(500, "failed to hash password")),
        );
    };
    let user_id = Uuid::new_v4();

    match dao::save_user(pool, user_id, username.clone(), password_hash).await {
        Ok(uid) => {
            let Ok(token) = create_token(uid, config) else {
                tracing::error!("token creation failed for new user {uid}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(Res::error(500, "failed to create token")),
                );
            };
            tracing::info!("user {username} registered (uid={uid})");
            (StatusCode::OK, Json(Res::success(token, "user created")))
        }
        Err(e) => {
            if e.to_string().contains("unique constraint") {
                tracing::info!("register conflict: username {username} already exists");
                return (
                    StatusCode::CONFLICT,
                    Json(Res::error(409, "user already exists")),
                );
            }
            // dao already logged this at error level
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, "register failed")),
            )
        }
    }
}

pub async fn login_user(
    pool: &PgPool,
    config: &Config,
    username: String,
    password: String,
) -> (StatusCode, Json<Res<String>>) {
    if username.is_empty() || password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(Res::error(400, "user name or password is empty")),
        );
    }

    let user = match dao::find_user_by_username(pool, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            tracing::warn!("login failed: user {username} not found");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "invalid username or password")),
            );
        }
        Err(_e) => {
            // dao already logged at error level
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, "internal error")),
            );
        }
    };

    match bcrypt::verify(&password, &user.password_hash) {
        Ok(true) => {}
        _ => {
            tracing::warn!("login failed: wrong password for user {username}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "invalid username or password")),
            );
        }
    }

    let token = match create_token(user.id, config) {
        Ok(t) => {
            tracing::info!("user {username} logged in (uid={})", user.id);
            t
        }
        Err(_) => {
            tracing::error!("token creation failed for user {} ({})", username, user.id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, "token generation failed")),
            );
        }
    };

    (StatusCode::OK, Json(Res::success(token, "login success")))
}

pub(crate) async fn logout_user(
    pool: &PgPool,
    username: String,
    password: String,
) -> Result<Uuid, (StatusCode, Json<Res<String>>)> {
    if username.is_empty() || password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(Res::error(400, "user name or password is empty")),
        ));
    }

    let user = match dao::find_user_by_username(pool, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            tracing::warn!("logout failed: user {username} not found");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "invalid username or password")),
            ));
        }
        Err(_e) => {
            // dao already logged at error level
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, "internal error")),
            ));
        }
    };

    match bcrypt::verify(&password, &user.password_hash) {
        Ok(true) => {}
        _ => {
            tracing::warn!("logout failed: wrong password for user {username}");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "invalid username or password")),
            ));
        }
    }

    tracing::info!("user {username} logged out (uid={})", user.id);
    Ok(user.id)
}

/// Delete a user account and all associated data.
/// Returns the deleted user's id so the caller can kick active sessions.
pub async fn delete_user(
    pool: &PgPool,
    config: &Config,
    token: &str,
) -> Result<Uuid, (StatusCode, Json<Res<String>>)> {
    let claims = match crate::jwt::verify_token(token, config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("delete_user: invalid token: {e}");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "invalid token")),
            ));
        }
    };

    match dao::delete_user(pool, claims.user_id).await {
        Ok(()) => Ok(claims.user_id),
        Err(e) => {
            tracing::error!("delete_user failed for uid={}: {e}", claims.user_id);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, "failed to delete account")),
            ))
        }
    }
}
