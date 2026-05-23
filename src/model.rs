use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::time::OffsetDateTime};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, FromRow, Deserialize, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    // pub nickname: Option<String>,
    // pub avatar: Option<String>,
    // pub online: bool,
    pub created_at: Option<OffsetDateTime>,
}
// Register request body
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 20, message = "用户名长度 3-20 位"))]
    pub username: String,
    #[validate(length(min = 6, max = 20, message = "密码长度 6-20 位"))]
    pub password: String,
}

// Login request body
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 20, message = "用户名长度 3-20 位"))]
    pub username: String,
    #[validate(length(min = 6, max = 20, message = "密码长度 6-20 位"))]
    pub password: String,
}

// Login response (returns token)
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
}

// Unified response format
#[derive(Serialize)]
pub struct Res<T> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
}

impl<T> Res<T> {
    pub fn success(data: T, msg: &str) -> Self {
        Self {
            code: 200,
            msg: msg.to_owned(),
            data: Some(data),
        }
    }

    pub fn error(code: i32, msg: &str) -> Self {
        Self {
            code,
            msg: msg.to_owned(),
            data: None,
        }
    }
}

// #[derive(Debug, Deserialize, Validate)]
// pub struct RegisterDTO {
//     #[validate(length(min = 3, max = 20, message = "用户名长度 3-20 位"))]
//     pub username: String,

//     #[validate(length(min = 6, max = 20, message = "密码长度 6-20 位"))]
//     pub password: String,

//     pub nickname: Option<String>,
// }

// use axum::extract::ws::Utf8Bytes;

/// Top-level unified message
#[derive(Debug, Serialize, Deserialize)]
pub struct WsMessage {
    pub cmd: String,
    pub seq: u64,
    pub data: serde_json::Value,
}

/// Private chat upstream
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivateChatReq {
    pub to_uid: Uuid,
    pub content: String,
    pub msg_type: u8,
    pub extra: Option<String>,
}

/// Private chat downstream push
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivatePushMsg {
    pub from_uid: Uuid,
    pub to_uid: Uuid,
    pub content: String,
    pub msg_type: u8,
    pub send_time: u64,
}

/// Private chat send confirmation
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivateChatAck {
    pub msg_id: i64,
    pub send_time: u64,
    pub delivered: bool,
}

/// Notify sender that previously-undelivered messages are now delivered
#[derive(Debug, Serialize, Deserialize)]
pub struct DeliveryUpdate {
    pub msg_ids: Vec<i64>,
    pub to_uid: Uuid,
}

/// Client requests marking messages from a peer as delivered
#[derive(Debug, Serialize, Deserialize)]
pub struct MarkDeliveredReq {
    pub peer_uid: Uuid,
}

/// Group chat upstream
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChatReq {
    pub group_id: u64,
    pub content: String,
    pub msg_type: u8,
}

/// Group chat downstream push
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupPushMsg {
    pub group_id: u64,
    pub from_uid: u64,
    pub from_nick: String,
    pub content: String,
    pub send_time: u64,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorReply {
    pub code: u16,
    pub msg: String,
}

/// Chat history query parameters
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub token: String,
    pub peer_uid: Uuid,
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

/// Generic query parameter with only token
#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    pub token: String,
}

/// Chat history entry
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ChatHistoryItem {
    pub msg_id: i64,
    pub from_uid: Uuid,
    pub to_uid: Uuid,
    pub content: String,
    pub msg_type: i16,
    pub send_time: i64, // Unix timestamp in milliseconds
}

/// Conversation list item
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ConversationItem {
    pub peer_uid: Uuid,
    pub peer_name: String,
    pub last_msg: String,
    pub last_msg_type: i16,
    pub last_time: i64,
    pub last_msg_id: i64,
}

/// User search parameters
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub token: String,
    pub q: String,
    pub limit: Option<i64>,
}

/// User search result
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct UserSearchItem {
    pub id: Uuid,
    pub username: String,
}
