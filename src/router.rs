use axum::{
    Json, Router,
    extract::{Query, State, WebSocketUpgrade, ws::{Message, Utf8Bytes, WebSocket}},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::dao;
use crate::jwt::verify_token;
use crate::model::{LoginRequest, PrivateChatReq, RegisterRequest, WsMessage};
use crate::service;
use crate::state::AppState;

pub fn app_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/im/ws", get(websocket_handler))
        .route("/api/user/register", post(user_register_handler))
        .route("/api/user/login", post(user_login_handler))
        .with_state(state)
}

// ==================== Page ====================

async fn index_handler() -> Html<&'static str> {
    Html(std::include_str!("../chat.html"))
}

// ==================== WebSocket ====================

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("WS auth failed: {e}");
            return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
        }
    };
    ws.on_upgrade(move |socket| handle_im_websocket(socket, claims.user_id, state))
}

async fn handle_im_websocket(socket: WebSocket, user_id: Uuid, state: Arc<AppState>) {
    tracing::debug!("handle_im_websocket user={user_id}");

    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    state.insert_online_user(user_id, tx);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let _ = sender.send(Message::Text(msg)).await;
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(raw) => {
                handle_biz_msg(user_id, raw.as_str(), state.clone()).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn handle_biz_msg(uid: Uuid, text: &str, state: Arc<AppState>) {
    let ws_msg: WsMessage = serde_json::from_str(text).expect("failed to parse WsMessage");
    match ws_msg.cmd.as_str() {
        "heartbeat" => {}
        "private_chat" => {
            let req: PrivateChatReq =
                serde_json::from_value(ws_msg.data).expect("failed to parse private_chat request");

            let _ = dao::save_message(&state.pg_pool, uid, req.to_uid, &req.content, req.msg_type)
                .await;
            if req.msg_type == 1 {
                state.send_to_user(req.to_uid, Utf8Bytes::from(req.content));
            }
        }
        _ => {}
    }
}

// ==================== User handlers ====================

async fn user_register_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    service::register_user(&state.pg_pool, &state.config, req.username, req.password).await
}

async fn user_login_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    service::login_user(&state.pg_pool, &state.config, req.username, req.password).await
}
