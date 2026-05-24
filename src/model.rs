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
    /// Command type: "private_chat", "group_chat", "heartbeat", "typing", "mark_delivered", "kicked"
    pub cmd: String,
    /// Client-assigned sequence number for request-response correlation
    pub seq: u64,
    /// Command-specific payload (PrivateChatReq, GroupChatReq, etc.)
    pub data: serde_json::Value,
}

/// Private chat upstream (client → server via WS)
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivateChatReq {
    pub to_uid: Uuid,
    pub content: String,
    /// Message type: 1 = text
    pub msg_type: u8,
    pub extra: Option<String>,
    /// Client-generated unique ID for idempotency (ON CONFLICT DO NOTHING)
    pub client_msg_id: Option<String>,
}

/// Private chat downstream push (server → recipient via WS)
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivatePushMsg {
    pub from_uid: Uuid,
    /// Sender's username (looked up from DB, included so frontend doesn't need a separate query)
    pub from_name: String,
    pub to_uid: Uuid,
    pub content: String,
    pub msg_type: u8,
    /// Unix timestamp in milliseconds when the message was saved
    pub send_time: u64,
}

/// Private chat send confirmation (server → sender via WS)
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivateChatAck {
    /// Database-assigned message ID
    pub msg_id: i64,
    /// Server-assigned send time (ms)
    pub send_time: u64,
    /// true = forwarded to online recipient, false = recipient offline (saved pending delivery)
    pub delivered: bool,
}

/// Notify sender that previously-undelivered messages are now delivered
/// (sent when recipient opens the chat, triggering mark_delivered)
#[derive(Debug, Serialize, Deserialize)]
pub struct DeliveryUpdate {
    /// IDs of messages that are now marked delivered
    pub msg_ids: Vec<i64>,
    /// The recipient who read them, so frontend knows which conversation to update
    pub to_uid: Uuid,
}

/// Group chat upstream (client → server via WS)
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChatReq {
    pub group_id: Uuid,
    pub content: String,
    pub msg_type: u8,
    /// Client-generated unique ID for idempotency
    pub client_msg_id: Option<String>,
}

/// Group chat send confirmation (server → sender via WS)
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChatAck {
    pub msg_id: i64,
    pub send_time: u64,
    /// Number of online group members who received the push
    pub online_count: usize,
}

/// Group chat downstream push (server → all online group members except sender)
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupPushMsg {
    pub group_id: Uuid,
    pub from_uid: Uuid,
    /// Sender's username (from DB join)
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
    /// Pagination cursor: return messages with msg_id < this value (exclusive)
    pub before: Option<i64>,
    /// Max messages to return (default 50, max 100)
    pub limit: Option<i64>,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorReply {
    pub code: u16,
    pub msg: String,
}

/// Chat history query parameters (GET /api/message/history)
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub token: String,
    pub peer_uid: Uuid,
    /// Pagination cursor: return messages with msg_id < this value (exclusive)
    pub before: Option<i64>,
    /// Max messages to return (default 50, max 100)
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

/// Chat history entry (returned by GET /api/message/history)
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ChatHistoryItem {
    /// Database message ID (BIGSERIAL), used as pagination cursor
    pub msg_id: i64,
    pub from_uid: Uuid,
    pub to_uid: Uuid,
    pub content: String,
    /// 1 = text
    pub msg_type: i16,
    /// Unix timestamp in milliseconds (EXTRACT EPOCH from created_at)
    pub send_time: i64,
}

/// Conversation list item (returned by GET /api/conversations — DISTINCT ON peer with latest msg)
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ConversationItem {
    pub peer_uid: Uuid,
    pub peer_name: String,
    pub last_msg: String,
    pub last_msg_type: i16,
    pub last_time: i64,
    pub last_msg_id: i64,
}

/// User search parameters (GET /api/user/search)
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub token: String,
    /// Search keyword (ILIKE %q% match on username)
    pub q: String,
    /// Max results to return (default 20, max 50)
    pub limit: Option<i64>,
}

/// User search result
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct UserSearchItem {
    pub id: Uuid,
    pub username: String,
}
