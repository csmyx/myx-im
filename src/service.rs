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
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Res::error(500, "failed to hash password")),
        );
    };
    let user_id = Uuid::new_v4();

    match dao::save_user(pool, user_id, username, password_hash).await {
        Ok(uid) => {
            let Ok(token) = create_token(uid, config) else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(Res::error(500, "failed to create token")),
                );
            };
            (StatusCode::OK, Json(Res::success(token, "user created")))
        }
        Err(e) => {
            if e.to_string().contains("unique constraint") {
                return (
                    StatusCode::CONFLICT,
                    Json(Res::error(409, "user already exists")),
                );
            }
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
        Ok(user) => user,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, &e.to_string())),
            );
        }
    };

    match bcrypt::verify(&password, &user.password_hash) {
        Ok(true) => {}
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "password is incorrect")),
            );
        }
    }

    let token = match create_token(user.id, config) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, "token generation failed")),
            );
        }
    };

    (StatusCode::OK, Json(Res::success(token, "login success")))
}
