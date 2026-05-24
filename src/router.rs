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
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::dao;
use crate::jwt::verify_token;
use crate::model::{
    ChatHistoryItem, ConversationItem, CreateGroupReq, DeleteRequest, DeliveryUpdate,
    GroupActionReq, GroupChatAck, GroupChatReq, GroupHistoryItem, GroupHistoryQuery, GroupInfo,
    GroupMember, GroupPushMsg, GroupQuery, HistoryQuery, LoginRequest, PrivateChatAck,
    PrivateChatReq, PrivatePushMsg, RegisterRequest, Res, SearchQuery, TokenQuery, UserSearchItem,
    WsMessage,
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
        .route("/api/user/delete", post(user_delete_handler))
        .route("/api/message/history", get(message_history_handler))
        .route("/api/conversations", get(conversations_handler))
        .route("/api/user/search", get(user_search_handler))
        .route("/api/group/create", post(group_create_handler))
        .route("/api/group/join", post(group_join_handler))
        .route("/api/group/leave", post(group_leave_handler))
        .route("/api/group/list", get(group_list_handler))
        .route("/api/group/members", get(group_members_handler))
        .route("/api/group/history", get(group_history_handler))
        .route("/debug", get(debug_handler))
        .with_state(state)
}

// ==================== Page ====================

async fn index_handler() -> Html<&'static str> {
    Html(std::include_str!("../chat.html"))
}

async fn debug_handler() -> Html<&'static str> {
    Html(std::include_str!("../debug.html"))
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
    let alive = Arc::new(AtomicBool::new(true));
    state.insert_online_user(user_id, tx.clone(), alive.clone());

    // Sync undelivered messages on connect
    {
        let tx = tx.clone();
        let pool = state.pg_pool.clone();
        tokio::spawn(async move {
            match dao::get_unseen_messages(&pool, user_id, 200).await {
                Ok(msgs) => {
                    // Collect sender UIDs and lookup usernames
                    let sender_uids: Vec<Uuid> = msgs.iter().map(|m| m.from_uid).collect();
                    let names: std::collections::HashMap<Uuid, String> = if !sender_uids.is_empty()
                    {
                        sqlx::query!(
                            "SELECT id, username FROM im_users WHERE id = ANY($1)",
                            &sender_uids,
                        )
                        .fetch_all(&pool)
                        .await
                        .map(|rows| rows.into_iter().map(|r| (r.id, r.username)).collect())
                        .unwrap_or_default()
                    } else {
                        std::collections::HashMap::new()
                    };
                    for msg in &msgs {
                        let push = PrivatePushMsg {
                            from_uid: msg.from_uid,
                            from_name: names.get(&msg.from_uid).cloned().unwrap_or_default(),
                            to_uid: msg.to_uid,
                            content: msg.content.clone(),
                            msg_type: msg.msg_type as u8,
                            send_time: msg.send_time as u64,
                        };
                        if let Ok(json) = serde_json::to_string(&push) {
                            let _ = tx.send(Utf8Bytes::from(format!(
                                r#"{{"cmd":"private_push","seq":0,"data":{}}}"#,
                                json
                            )));
                        }
                    }
                }
                Err(e) => tracing::error!("get_unseen_messages failed: {e}"),
            }
        });
    }

    // Spawn a task that forwards messages from the mpsc channel to the WS sender.
    // When the WS write side dies (client disconnects), we break the loop.
    // The `alive` flag is set to false so that send_to_user() can immediately
    // detect the dead connection without waiting for mpsc send to fail.
    let alive_fwd = alive.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
        alive_fwd.store(false, Ordering::Release);
        tracing::debug!("forwarding task exited for user={user_id}");
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

    // Mark this connection as dead. If we were kicked by a duplicate login,
    // the new connection already replaced our entry in the map, so this is
    // a no-op for the new connection's entry. The forwarding task also sets
    // alive=false on exit, so this is defense-in-depth.
    alive.store(false, Ordering::Release);

    // NOTE: We intentionally do NOT call state.remove_online_user(user_id) here.
    // If this connection was kicked by a duplicate login, remove_online_user would
    // delete the NEW connection's entry, not ours. Dead entries are cleaned
    // lazily by send_to_user() when the forwarding task's `rx` gets dropped.
    tracing::info!("WS disconnected: user={user_id}");
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
        "typing" => {
            // Forward typing indicator to peer
            if let Ok(req) = serde_json::from_value::<PrivateChatReq>(ws_msg.data) {
                let payload = format!(
                    r#"{{"cmd":"typing","seq":{},"data":{{"from_uid":"{}"}}}}"#,
                    ws_msg.seq, uid,
                );
                state.send_to_user(req.to_uid, Utf8Bytes::from(payload));
            }
        }
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

            match dao::persist_chat_message(
                &state.pg_pool,
                uid,
                req.to_uid,
                &req.content,
                req.msg_type,
                req.client_msg_id.as_deref(),
            )
            .await
            {
                Ok(msg_id) => {
                    // Look up sender username for the push
                    let from_name: String = sqlx::query_scalar!(
                        "SELECT username FROM im_users WHERE id = $1",
                        uid as Uuid,
                    )
                    .fetch_one(&state.pg_pool)
                    .await
                    .unwrap_or_default();

                    // Push to recipient first to know delivery status
                    let push = PrivatePushMsg {
                        from_uid: uid,
                        from_name,
                        to_uid: req.to_uid,
                        content: req.content.clone(),
                        msg_type: req.msg_type,
                        send_time,
                    };
                    let delivered = if let Ok(json) = serde_json::to_string(&push) {
                        state.send_to_user(
                            req.to_uid,
                            Utf8Bytes::from(format!(
                                r#"{{"cmd":"private_push","seq":{},"data":{}}}"#,
                                ws_msg.seq, json
                            )),
                        )
                    } else {
                        tracing::error!("failed to serialize push msg to user={}", req.to_uid);
                        false
                    };

                    // ACK to sender with delivery status
                    let ack = PrivateChatAck {
                        msg_id,
                        send_time,
                        delivered,
                    };
                    if let Ok(json) = serde_json::to_string(&ack) {
                        let _ = tx.send(Utf8Bytes::from(format!(
                            r#"{{"cmd":"private_chat_ack","seq":{},"data":{}}}"#,
                            ws_msg.seq, json,
                        )));
                    } else {
                        tracing::error!("failed to serialize ACK for msg_id={msg_id}");
                    }

                    // Mark unseen messages from recipient→sender as seen
                    // (replying implies the sender read the recipient's messages)
                    match dao::get_unseen_ids_from_peer(&state.pg_pool, req.to_uid, uid).await {
                        Ok(ids) if !ids.is_empty() => {
                            if let Err(e) = dao::mark_messages_seen(&state.pg_pool, &ids).await {
                                tracing::error!("mark_messages_seen on reply failed: {e}");
                            } else {
                                let update = DeliveryUpdate {
                                    msg_ids: ids,
                                    to_uid: uid,
                                };
                                if let Ok(json) = serde_json::to_string(&update) {
                                    state.send_to_user(
                                        req.to_uid,
                                        Utf8Bytes::from(format!(
                                            r#"{{"cmd":"delivery_update","seq":0,"data":{}}}"#,
                                            json
                                        )),
                                    );
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => tracing::error!("get_unseen_ids_from_peer on reply failed: {e}"),
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
        "group_chat" => {
            let req: GroupChatReq = match serde_json::from_value(ws_msg.data) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("failed to parse group_chat request: {e}");
                    return;
                }
            };

            // Verify membership
            match dao::is_group_member(&state.pg_pool, req.group_id, uid).await {
                Ok(true) => {}
                Ok(false) => {
                    let _ = tx.send(Utf8Bytes::from(format!(
                        r#"{{"cmd":"error","seq":{},"data":{{"code":403,"msg":"not a member of this group"}}}}"#,
                        ws_msg.seq,
                    )));
                    return;
                }
                Err(e) => {
                    tracing::error!("is_group_member failed: {e}");
                    return;
                }
            }

            let send_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            match dao::persist_group_message(
                &state.pg_pool,
                req.group_id,
                uid,
                &req.content,
                req.msg_type,
                req.client_msg_id.as_deref(),
            )
            .await
            {
                Ok(msg_id) => {
                    // ACK to sender
                    let ack = GroupChatAck {
                        msg_id,
                        send_time,
                        online_count: 0,
                    };
                    if let Ok(json) = serde_json::to_string(&ack) {
                        let _ = tx.send(Utf8Bytes::from(format!(
                            r#"{{"cmd":"group_chat_ack","seq":{},"data":{}}}"#,
                            ws_msg.seq, json,
                        )));
                    }

                    // Push to online group members (except sender)
                    match dao::get_group_member_uids(&state.pg_pool, req.group_id).await {
                        Ok(members) => {
                            let push = GroupPushMsg {
                                group_id: req.group_id,
                                from_uid: uid,
                                from_name: String::new(),
                                content: req.content.clone(),
                                msg_type: req.msg_type,
                                send_time,
                            };
                            if let Ok(json) = serde_json::to_string(&push) {
                                let payload = Utf8Bytes::from(format!(
                                    r#"{{"cmd":"group_push","seq":0,"data":{}}}"#,
                                    json
                                ));
                                for member_uid in &members {
                                    if *member_uid != uid {
                                        state.send_to_user(*member_uid, payload.clone());
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::error!("get_group_member_uids failed: {e}"),
                    }
                }
                Err(e) => {
                    tracing::error!("save_group_message failed: {e}");
                    let _ = tx.send(Utf8Bytes::from(format!(
                        r#"{{"cmd":"error","seq":{},"data":{{"code":500,"msg":"send group message failed"}}}}"#,
                        ws_msg.seq,
                    )));
                }
            }
        }
        other => {
            tracing::warn!("unknown WS command: {other} from user={uid}");
        }
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
        Ok((items, seen_ids)) => {
            // Notify the peer (message sender) that their messages were seen.
            // seen_ids are the messages from peer→caller that were just marked seen=TRUE.
            if !seen_ids.is_empty() {
                let update = DeliveryUpdate {
                    msg_ids: seen_ids,
                    to_uid: claims.user_id,
                };
                if let Ok(json) = serde_json::to_string(&update) {
                    state.send_to_user(
                        query.peer_uid,
                        axum::extract::ws::Utf8Bytes::from(format!(
                            r#"{{"cmd":"delivery_update","seq":0,"data":{}}}"#,
                            json
                        )),
                    );
                }
            }
            (StatusCode::OK, Json(Res::success(items, "ok"))).into_response()
        }
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

async fn user_delete_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteRequest>,
) -> impl IntoResponse {
    match service::delete_user(&state.pg_pool, &state.config, &req.token).await {
        Ok(uid) => {
            // Kick any active WebSocket sessions for this user
            state.send_to_user(
                uid,
                axum::extract::ws::Utf8Bytes::from_static(
                    r#"{"cmd":"kicked","seq":0,"data":{"msg":"account deleted"}}"#,
                ),
            );
            (
                StatusCode::OK,
                Json(Res::success("ok".to_string(), "account deleted")),
            )
                .into_response()
        }
        Err(err) => err.into_response(),
    }
}

// ==================== Group handlers ====================

async fn group_create_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGroupReq>,
) -> impl IntoResponse {
    let claims = match verify_token(&req.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("group create auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<GroupInfo>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    if req.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(Res::<GroupInfo>::error(400, "group name required")),
        )
            .into_response();
    }

    match dao::create_group(&state.pg_pool, req.name.trim(), claims.user_id).await {
        Ok(group) => (StatusCode::OK, Json(Res::success(group, "group created"))).into_response(),
        Err(e) => {
            tracing::error!("create_group failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<GroupInfo>::error(500, "create group failed")),
            )
                .into_response()
        }
    }
}

async fn group_join_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GroupActionReq>,
) -> impl IntoResponse {
    let claims = match verify_token(&req.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("group join auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<()>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    match dao::join_group(&state.pg_pool, req.group_id, claims.user_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(Res::success("".to_string(), "joined group")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("join_group failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<()>::error(500, "join group failed")),
            )
                .into_response()
        }
    }
}

async fn group_leave_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GroupActionReq>,
) -> impl IntoResponse {
    let claims = match verify_token(&req.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("group leave auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<()>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    match dao::leave_group(&state.pg_pool, req.group_id, claims.user_id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(Res::success("".to_string(), "left group")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("leave_group failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<()>::error(500, "leave group failed")),
            )
                .into_response()
        }
    }
}

async fn group_list_handler(
    Query(query): Query<TokenQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("group list auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<Vec<GroupInfo>>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    match dao::list_my_groups(&state.pg_pool, claims.user_id).await {
        Ok(groups) => (StatusCode::OK, Json(Res::success(groups, "ok"))).into_response(),
        Err(e) => {
            tracing::error!("list_my_groups failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<Vec<GroupInfo>>::error(500, "list groups failed")),
            )
                .into_response()
        }
    }
}

async fn group_members_handler(
    Query(query): Query<GroupQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let _claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("group members auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<Vec<GroupMember>>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    match dao::list_group_members(&state.pg_pool, query.group_id).await {
        Ok(members) => (StatusCode::OK, Json(Res::success(members, "ok"))).into_response(),
        Err(e) => {
            tracing::error!("list_group_members failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<Vec<GroupMember>>::error(500, "list members failed")),
            )
                .into_response()
        }
    }
}

async fn group_history_handler(
    Query(query): Query<GroupHistoryQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let _claims = match verify_token(&query.token, &state.config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("group history auth failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(Res::<Vec<GroupHistoryItem>>::error(401, "invalid token")),
            )
                .into_response();
        }
    };

    let limit = query.limit.unwrap_or(50).min(100);

    match dao::get_group_history(&state.pg_pool, query.group_id, query.before, limit).await {
        Ok(items) => (StatusCode::OK, Json(Res::success(items, "ok"))).into_response(),
        Err(e) => {
            tracing::error!("get_group_history failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Res::<Vec<GroupHistoryItem>>::error(
                    500,
                    "query history failed",
                )),
            )
                .into_response()
        }
    }
}
