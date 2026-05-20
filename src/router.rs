use super::{
    model::{LoginReq, WsMsg},
    service::{get_user_id_by_token, login, send_single_msg},
    state::OnlineUsers,
};

use axum::{
    Router,
    extract::ws::{WebSocket, WebSocketUpgrade},
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::PgPool;

// ======================
// 1. HTTP路由（管理接口，短连接，无状态）
// ======================
pub fn http_routes() -> Router<PgPool> {
    Router::new()
        .route("/api/login", post(login_handler))
        .route("/api/friend/list", get(friend_list_handler))
}

// HTTP登录处理函数
async fn login_handler(
    State(pool): State<PgPool>,
    axum::Json(req): axum::Json<LoginReq>,
) -> (StatusCode, String) {
    match login(&pool, req).await {
        Ok(resp) => (StatusCode::OK, serde_json::to_string(&resp).unwrap()),
        Err(_) => (StatusCode::BAD_REQUEST, "登录失败".into()),
    }
}

// ======================
// 2. WebSocket路由（实时接口，长连接）
// ======================
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    token: String, // 连接时携带HTTP登录拿到的token
}

pub fn ws_routes(online_users: OnlineUsers) -> Router<PgPool> {
    Router::new()
        .route("/api/ws", get(ws_upgrade_handler))
        .with_state(online_users)
}

// WebSocket握手处理：校验token → 拿到user_id → 保存全局连接
async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(pool): State<PgPool>,
    State(online_users): State<OnlineUsers>,
) -> impl IntoResponse {
    // 1. 校验token，和HTTP登录用同一套逻辑，拿到用户ID
    let user_id = get_user_id_by_token(&query.token).unwrap();

    // 2. 升级WebSocket，保存连接
    ws.on_upgrade(move |socket| handle_ws_connection(user_id, socket, online_users, pool))
}

// WebSocket长连接循环：收发消息
async fn handle_ws_connection(
    user_id: u64,
    mut socket: WebSocket,
    online_users: OnlineUsers,
    pool: PgPool,
) {
    // 【关键】用户上线：把连接存入全局Map
    online_users.write().await.insert(user_id, socket.clone());

    // 循环接收客户端消息
    while let Some(Ok(msg)) = socket.next().await {
        if let axum::extract::ws::Message::Text(text) = msg {
            let ws_msg: WsMsg = serde_json::from_str(&text).unwrap();
            // 调用service发送消息
            send_single_msg(&online_users, user_id, ws_msg)
                .await
                .unwrap();
        }
    }

    // 【关键】用户下线：从全局Map删除连接
    online_users.write().await.remove(&user_id);
}
