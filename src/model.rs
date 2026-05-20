use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use validator::Validate;

// HTTP登录请求DTO
// #[derive(Debug, Deserialize)]
// pub struct LoginReq {
//     pub username: String,
//     pub password: String,
// }

// // HTTP登录响应：返回Token
// #[derive(Debug, Serialize)]
// pub struct LoginResp {
//     pub token: String,
//     pub user_id: u64,
// }

// // WebSocket收发消息DTO（和HTTP完全独立）
// #[derive(Debug, Serialize, Deserialize)]
// pub struct WsMsg {
//     pub msg_type: u8,   // 1单聊 2群聊
//     pub target_id: u64, // 接收方ID
//     pub content: String,
// }

// // 自定义错误
// #[derive(Debug, Error)]
// pub enum ImError {
//     #[error("登录失败")]
//     LoginFail,
//     #[error("Token无效")]
//     InvalidToken,
//     #[error("用户不在线")]
//     UserOffline,
// }

// #[derive(Debug, FromRow, Serialize, Clone)]
// pub struct User {
//     pub id: i64,
//     pub username: String,
//     pub password: String, // 存加密后的密码
//     pub nickname: Option<String>,
//     pub avatar: Option<String>,
//     pub online: bool,
//     // pub create_at: chrono::DateTime<chrono::Utc>,
// }

// #[derive(Debug, Deserialize, Validate)]
// pub struct RegisterDTO {
//     #[validate(length(min = 3, max = 20, message = "用户名长度 3-20 位"))]
//     pub username: String,

//     #[validate(length(min = 6, max = 20, message = "密码长度 6-20 位"))]
//     pub password: String,

//     pub nickname: Option<String>,
// }

// use axum::extract::ws::Utf8Bytes;

/// 顶层统一消息体
#[derive(Debug, Serialize, Deserialize)]
pub struct WsMessage {
    pub cmd: String,
    pub seq: u64,
    pub data: serde_json::Value,
}

/// 私聊上行
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivateChatReq {
    pub to_uid: u64,
    pub content: String,
    pub msg_type: u8,
    pub extra: Option<String>,
}

/// 私聊下行推送
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivatePushMsg {
    pub from_uid: u64,
    pub to_uid: u64,
    pub content: String,
    pub msg_type: u8,
    pub send_time: u64,
}

/// 群聊上行
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChatReq {
    pub group_id: u64,
    pub content: String,
    pub msg_type: u8,
}

/// 群聊下行推送
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupPushMsg {
    pub group_id: u64,
    pub from_uid: u64,
    pub from_nick: String,
    pub content: String,
    pub send_time: u64,
}

/// 错误响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorReply {
    pub code: u16,
    pub msg: String,
}
