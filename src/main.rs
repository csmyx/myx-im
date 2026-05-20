use axum::{
    Router,
    extract::{
        FromRef, FromRequestParts, Path, State, WebSocketUpgrade,
        ws::{Message, Utf8Bytes, WebSocket},
    },
    http::{StatusCode, request::Parts},
    response::{Html, IntoResponse},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use myx_im::{
    model::{PrivateChatReq, WsMessage},
    state::{AppState, init_app_state},
};
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::{net::TcpListener, sync::mpsc};
use tracing_subscriber::{filter::targets, layer::SubscriberExt, util::SubscriberInitExt};

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
        .route("/im/ws/{uid}", get(websocket_handler))
        // ==================== 用户模块 API ====================
        // .route("/api/user/register", post(user_register_handler))
        // .route("/api/user/login", post(user_login_handler))
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

// This function deals with a single websocket connection, i.e., a single
// connected client / user, for which we will spawn two independent tasks (for
// receiving / sending chat messages).
async fn websocket_handler(
    ws: WebSocketUpgrade,
    Path(uid): Path<u64>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("websocket_handler");
    ws.on_upgrade(move |socket| handle_im_websocket(socket, uid, state))
}

async fn handle_im_websocket(mut socket: WebSocket, uid: u64, state: Arc<AppState>) {
    tracing::debug!("handle_im_websocket");

    // By splitting, we can send and receive at the same time.
    let (mut sender, mut receiver) = socket.split();

    let (tx, mut rx) = mpsc::unbounded_channel();
    state.insert_online_user(uid, tx);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // tracing::debug!("send to client: {msg:?}");
            sender.send(Message::Text(msg)).await.unwrap();
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            // 业务文本消息：走你自定义 JSON 协议
            Message::Text(raw) => {
                let text = raw.as_str();
                // 统一解析你的 cmd 消息协议
                handle_biz_msg(uid, text, state.clone()).await;
            }

            // 底层 Ping：框架自动回复 Pong，你不用管
            Message::Ping(_) => {}
            Message::Pong(_) => {}

            // 连接关闭
            Message::Close(_) => {
                // 下线清理逻辑
                // state.offline_user(uid);
                break;
            }
            _ => {}
        }
    }
    todo!()
}

async fn handle_biz_msg(uid: u64, text: &str, state: Arc<AppState>) {
    let ws_msg: WsMessage = serde_json::from_str(&text).expect("failed to parse WsMessage");
    match ws_msg.cmd.as_str() {
        "hearbeat" => {}
        "private_chat" => {
            let req: PrivateChatReq =
                serde_json::from_value(ws_msg.data).expect("failed to parse private_chat request");

            state
                .save_message(uid, req.to_uid, &req.content, req.msg_type)
                .await;
            match req.msg_type {
                1 => {
                    tracing::debug!("send to user: {}", req.content);
                    let content = Utf8Bytes::from(req.content);
                    state.send_to_user(req.to_uid, content);
                }
                2 => {
                    tracing::debug!("send to user: {}", req.content);
                    todo!()
                }
                _ => {}
            }
        }
        _ => {}
    }
}

// async fn push_to_user(state: Arc<AppState>, uid: u64, msg: &str) {
//     state.send_to_user(uid, msg);
// }

async fn index_handler() -> Html<&'static str> {
    Html(std::include_str!("../chat.html"))
}
