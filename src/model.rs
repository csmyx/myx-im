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
// 注册请求体
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 20, message = "用户名长度 3-20 位"))]
    pub username: String,
    #[validate(length(min = 6, max = 20, message = "密码长度 6-20 位"))]
    pub password: String,
}

// 登录请求体
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 20, message = "用户名长度 3-20 位"))]
    pub username: String,
    #[validate(length(min = 6, max = 20, message = "密码长度 6-20 位"))]
    pub password: String,
}

// 登录响应（返回token）
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
}

// 统一返回格式
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
    pub to_uid: Uuid,
    pub content: String,
    pub msg_type: u8,
    pub extra: Option<String>,
}

/// 私聊下行推送
#[derive(Debug, Serialize, Deserialize)]
pub struct PrivatePushMsg {
    pub from_uid: Uuid,
    pub to_uid: Uuid,
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
