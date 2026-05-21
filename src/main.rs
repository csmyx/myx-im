use axum::{
    Json, Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, Utf8Bytes, WebSocket},
    },
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use myx_im::{
    jwt::{create_token, verify_token},
    model::{LoginRequest, PrivateChatReq, RegisterRequest, Res, WsMessage},
    state::{AppState, init_app_state},
};
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_line_number(true)
                .with_file(true),
        )
        .init();

    let db_connection_str = std::env::var("DATABASE_URL")
        // .unwrap_or_else(|_| "postgres://postgres:password@localhost".to_string());
        .unwrap_or_else(|_| "postgres://postgres:123456@0.0.0.0:5432/myx_im".to_string());
    tracing::debug!("db connection string: {:?}", db_connection_str);

    // set up connection pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_connection_str)
        .await
        .expect("can't connect to database");

    let app_state = init_app_state(pool);

    let app = Router::new()
        // .route("/", get(index_handler))
        // .route("/websocket", get(websocket_handler))
        // .with_state(Arc::new(app_state));
        // ==================== 基础页面 ====================
        .route("/", get(index_handler))
        // ==================== WebSocket 长连接（IM 核心） ====================
        .route("/im/ws", get(websocket_handler))
        // ==================== 用户模块 API ====================
        .route("/api/user/register", post(user_register_handler))
        .route("/api/user/login", post(user_login_handler))
        // .route("/api/user/info", get(user_info_handler))
        // .route("/api/user/profile", put(user_update_profile_handler))
        // .route("/api/user/search", get(user_search_handler))
        // // ==================== 好友模块 API ====================
        // .route("/api/friend/apply", post(friend_apply_handler))
        // .route("/api/friend/handle", put(friend_handle_handler))
        // .route("/api/friend/list", get(friend_list_handler))
        // .route("/api/friend/{friend_id}", delete(friend_delete_handler))
        // // ==================== 群聊模块 API ====================
        // .route("/api/group/create", post(group_create_handler))
        // .route("/api/group/join", post(group_join_handler))
        // .route("/api/group/list", get(group_list_handler))
        // .route("/api/group/{group_id}/members", get(group_members_handler))
        // // ==================== 消息模块 API ====================
        // .route("/api/message/history", get(message_history_handler))
        // .route("/api/message/offline", get(message_offline_handler))
        // 全局状态（你的在线用户连接池 + DB 连接池）
        .with_state(Arc::new(app_state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    let _ = axum::serve(listener, app).await;
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("websocket_handler");
    // Verify JWT token before upgrading
    let claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("WS auth failed: {e}");
            return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
        }
    };
    let user_id = claims.user_id;
    ws.on_upgrade(move |socket| handle_im_websocket(socket, user_id, state))
}

async fn handle_im_websocket(socket: WebSocket, user_id: Uuid, state: Arc<AppState>) {
    tracing::debug!("handle_im_websocket user={user_id}");

    let (mut sender, mut receiver) = socket.split();

    let (tx, mut rx) = mpsc::unbounded_channel();
    state.insert_online_user(user_id, tx);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            sender.send(Message::Text(msg)).await.unwrap();
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(raw) => {
                handle_biz_msg(user_id, raw.as_str(), state.clone()).await;
            }
            Message::Ping(_) => {}
            Message::Pong(_) => {}
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }
}

async fn handle_biz_msg(uid: Uuid, text: &str, state: Arc<AppState>) {
    let ws_msg: WsMessage = serde_json::from_str(&text).expect("failed to parse WsMessage");
    match ws_msg.cmd.as_str() {
        "heartbeat" => {}
        "private_chat" => {
            let req: PrivateChatReq =
                serde_json::from_value(ws_msg.data).expect("failed to parse private_chat request");

            let _ = state
                .save_message(uid, req.to_uid, &req.content, req.msg_type)
                .await;
            match req.msg_type {
                1 => {
                    tracing::debug!("send to user: {}", req.content);
                    let content = Utf8Bytes::from(req.content);
                    state.send_to_user(req.to_uid, content);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

async fn user_register_handler(
    State(state): State<Arc<AppState>>,
    req: Json<RegisterRequest>,
) -> impl IntoResponse {
    let password = &req.password;
    let Ok(password_hash) = bcrypt::hash(password, bcrypt::DEFAULT_COST) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Res::error(500, "failed to hash password")),
        );
    };
    let user_name = req.username.to_owned();
    let user_id = Uuid::new_v4();

    match state.save_user(user_id, user_name, password_hash).await {
        Ok(uid) => {
            let Ok(token) = create_token(uid, &state.config) else {
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

async fn user_login_handler(
    State(state): State<Arc<AppState>>,
    req: Json<LoginRequest>,
) -> impl IntoResponse {
    // 1. basic validation
    if req.username.is_empty() || req.password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(Res::error(400, "user name or password is empty")),
        );
    }

    // 2. query user
    let user = match state.find_user_by_username(&req.username).await {
        Ok(user) => user,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::error(500, &e.to_string())),
            );
        }
    };

    // 3. verify password
    match bcrypt::verify(&req.password, &user.password_hash) {
        Ok(true) => {} // 密码正确
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::error(401, "password is incorrect")),
            );
        }
    }

    // 4. generate token
    let token = match create_token(user.id, &state.config) {
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

async fn index_handler() -> Html<&'static str> {
    Html(std::include_str!("../chat.html"))
}
