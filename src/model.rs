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
    #[validate(length(min = 3, max = 20, message = "Username length 3-20 characters"))]
    pub username: String,
    #[validate(length(min = 6, max = 20, message = "Password length 6-20 characters"))]
    pub password: String,
}

// Login request body
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 20, message = "Username length 3-20 characters"))]
    pub username: String,
    #[validate(length(min = 6, max = 20, message = "Password length 6-20 characters"))]
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
    pub client_msg_id: Option<String>,
}

/// Private chat downstream push
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivatePushMsg {
    pub from_uid: Uuid,
    pub from_name: String,
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

/// Notify sender that recipient has read up to a message
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadReceipt {
    pub peer_uid: Uuid,
    pub last_read_msg_id: i64,
}

/// Group chat upstream
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChatReq {
    pub group_id: Uuid,
    pub content: String,
    pub msg_type: u8,
    pub client_msg_id: Option<String>,
}

/// Group chat send confirmation
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChatAck {
    pub msg_id: i64,
    pub send_time: u64,
    pub online_count: usize,
}

/// Group chat downstream push
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupPushMsg {
    pub group_id: Uuid,
    pub from_uid: Uuid,
    pub from_name: String,
    pub content: String,
    pub msg_type: u8,
    pub send_time: u64,
}

/// Create group request
#[derive(Debug, Deserialize)]
pub struct CreateGroupReq {
    pub token: String,
    pub name: String,
}

/// Join/Leave group request
#[derive(Debug, Deserialize)]
pub struct GroupActionReq {
    pub token: String,
    pub group_id: Uuid,
}

/// Group info (for list)
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct GroupInfo {
    pub group_id: Uuid,
    pub name: String,
    pub owner_uid: Uuid,
    pub member_count: i64,
    pub created_at: Option<OffsetDateTime>,
}

/// Group member
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct GroupMember {
    pub user_id: Uuid,
    pub username: String,
}

/// Group history item
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct GroupHistoryItem {
    pub msg_id: i64,
    pub group_id: Uuid,
    pub from_uid: Uuid,
    pub from_name: String,
    pub content: String,
    pub msg_type: i16,
    pub send_time: i64,
}

/// Group members query
#[derive(Debug, Deserialize)]
pub struct GroupQuery {
    pub token: String,
    pub group_id: Uuid,
}

/// Group history query
#[derive(Debug, Deserialize)]
pub struct GroupHistoryQuery {
    pub token: String,
    pub group_id: Uuid,
    pub before: Option<i64>,
    pub limit: Option<i64>,
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

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
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
