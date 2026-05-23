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
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::dao;
use crate::jwt::verify_token;
use crate::model::{
    ChatHistoryItem, ConversationItem, HistoryQuery, LoginRequest, PrivateChatAck, PrivateChatReq,
    PrivatePushMsg, RegisterRequest, Res, SearchQuery, TokenQuery, UserSearchItem, WsMessage,
};
use crate::service;
use crate::state::AppState;

pub fn app_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/im/ws", get(websocket_handler))
        .route("/api/user/register", post(user_register_handler))
        .route("/api/user/login", post(user_login_handler))
        .route("/api/user/logout", post(user_logout_handler))
        .route("/api/message/history", get(message_history_handler))
        .route("/api/conversations", get(conversations_handler))
        .route("/api/user/search", get(user_search_handler))
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
    state.insert_online_user(user_id, tx.clone());

    // Sync undelivered messages on connect
    {
        let tx = tx.clone();
        let pool = state.pg_pool.clone();
        tokio::spawn(async move {
            match dao::get_undelivered_messages(&pool, user_id, 200).await {
                Ok(msgs) => {
                    let msg_ids: Vec<i64> = msgs.iter().map(|m| m.msg_id).collect();
                    for msg in &msgs {
                        let push = PrivatePushMsg {
                            from_uid: msg.from_uid,
                            to_uid: msg.to_uid,
                            content: msg.content.clone(),
                            msg_type: msg.msg_type as u8,
                            send_time: msg.send_time as u64,
                        };
                        if let Ok(json) = serde_json::to_string(&push) {
                            let _ = tx.send(Utf8Bytes::from(json));
                        }
                    }
                    if let Err(e) = dao::mark_messages_delivered(&pool, &msg_ids).await {
                        tracing::error!("mark_messages_delivered failed: {e}");
                    }
                }
                Err(e) => tracing::error!("get_undelivered_messages failed: {e}"),
            }
        });
    }

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let _ = sender.send(Message::Text(msg)).await;
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(raw) => {
                handle_biz_msg(user_id, raw.as_str(), state.clone(), &tx).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn handle_biz_msg(
    uid: Uuid,
    text: &str,
    state: Arc<AppState>,
    tx: &mpsc::UnboundedSender<Utf8Bytes>,
) {
    let ws_msg: WsMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("failed to parse WsMessage: {e}");
            let _ = tx.send(Utf8Bytes::from(
                r#"{"cmd":"error","seq":0,"data":{"code":400,"msg":"invalid message format"}}"#,
            ));
            return;
        }
    };
    match ws_msg.cmd.as_str() {
        "heartbeat" => {}
        "private_chat" => {
            let req: PrivateChatReq = match serde_json::from_value(ws_msg.data) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("failed to parse private_chat request: {e}");
                    let _ = tx.send(Utf8Bytes::from(format!(
                        r#"{{"cmd":"error","seq":{},"data":{{"code":400,"msg":"invalid private_chat data: {e}"}}}}"#,
                        ws_msg.seq,
                    )));
                    return;
                }
            };

            let send_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            match dao::save_message(&state.pg_pool, uid, req.to_uid, &req.content, req.msg_type)
                .await
            {
                Ok(msg_id) => {
                    // ACK to sender
                    let ack = PrivateChatAck { msg_id, send_time };
                    if let Ok(json) = serde_json::to_string(&ack) {
                        let _ = tx.send(Utf8Bytes::from(format!(
                            r#"{{"cmd":"private_chat_ack","seq":{},"data":{}}}"#,
                            ws_msg.seq, json,
                        )));
                    }

                    // Push to recipient
                    let push = PrivatePushMsg {
                        from_uid: uid,
                        to_uid: req.to_uid,
                        content: req.content,
                        msg_type: req.msg_type,
                        send_time,
                    };
                    if let Ok(json) = serde_json::to_string(&push) {
                        state.send_to_user(req.to_uid, Utf8Bytes::from(json));
                    }
                }
                Err(e) => {
                    tracing::error!("save_message failed: {e}");
                    let _ = tx.send(Utf8Bytes::from(format!(
                        r#"{{"cmd":"error","seq":{},"data":{{"code":500,"msg":"save message failed"}}}}"#,
                        ws_msg.seq,
                    )));
                }
            }
        }
        _ => {}
    }
}

// ==================== Message handlers ====================

async fn message_history_handler(
    Query(query): Query<HistoryQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("history auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<()>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    let limit = query.limit.unwrap_or(50).min(100);

    match dao::get_chat_history(
        &state.pg_pool,
        claims.user_id,
        query.peer_uid,
        query.before,
        limit,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(Res::success(items, "ok"))).into_response(),
        Err(e) => {
            tracing::error!("get_chat_history failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<Vec<ChatHistoryItem>>::error(
                    500,
                    "query history failed",
                )),
            )
                .into_response()
        }
    }
}

async fn conversations_handler(
    Query(query): Query<TokenQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("conversations auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<()>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    match dao::get_conversations(&state.pg_pool, claims.user_id).await {
        Ok(items) => (StatusCode::OK, Json(Res::success(items, "ok"))).into_response(),
        Err(e) => {
            tracing::error!("get_conversations failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<Vec<ConversationItem>>::error(
                    500,
                    "query conversations failed",
                )),
            )
                .into_response()
        }
    }
}

async fn user_search_handler(
    Query(query): Query<SearchQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("user search auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<()>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    let limit = query.limit.unwrap_or(20).min(50);

    match dao::search_users(&state.pg_pool, &query.q, claims.user_id, limit).await {
        Ok(items) => (StatusCode::OK, Json(Res::success(items, "ok"))).into_response(),
        Err(e) => {
            tracing::error!("search_users failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<Vec<UserSearchItem>>::error(
                    500,
                    "search users failed",
                )),
            )
                .into_response()
        }
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

async fn user_logout_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    match service::logout_user(&state.pg_pool, req.username, req.password).await {
        Ok(uid) => {
            state.remove_online_user(uid);
            (
                StatusCode::OK,
                Json(Res::success("".to_string(), "logout success")),
            )
                .into_response()
        }
        Err(err) => err.into_response(),
    }
}
