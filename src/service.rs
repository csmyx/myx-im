// use super::{
//     im_dao,
//     im_model::{ImError, LoginReq, LoginResp, WsMsg},
// };
// use sqlx::PgPool;
// use std::collections::HashMap;
// use uuid::Uuid;

// /// 【HTTP接口：登录】生成Token，返回给前端
// pub async fn login(pool: &PgPool, req: LoginReq) -> Result<LoginResp, ImError> {
//     // 1. 查库校验账号密码
//     let user = im_dao::get_user_by_name(pool, &req.username)
//         .await
//         .map_err(|_| ImError::LoginFail)?;
//     if user.password != req.password {
//         return Err(ImError::LoginFail);
//     }

//     // 2. 生成Token（实际用JWT，这里简化为UUID）
//     let token = Uuid::new_v4().to_string();
//     // 全局Token映射：token => user_id（全局保存）
//     TOKEN_MAP.insert(token.clone(), user.id);

//     Ok(LoginResp {
//         token,
//         user_id: user.id,
//     })
// }

// /// 【共用工具：Token校验】HTTP和WebSocket都调用这个函数
// pub fn get_user_id_by_token(token: &str) -> Result<u64, ImError> {
//     TOKEN_MAP.get(token).copied().ok_or(ImError::InvalidToken)
// }

// /// 【WebSocket业务：发送单聊消息】
// pub async fn send_single_msg(
//     online_users: &super::im_state::OnlineUsers,
//     sender_id: u64,
//     msg: WsMsg,
// ) -> Result<(), ImError> {
//     // 1. 查接收方是否在线
//     let online = online_users.read().await;
//     let target_conn = online.get(&msg.target_id).ok_or(ImError::UserOffline)?;

//     // 2. 推送消息给对方
//     let json = serde_json::to_string(&msg).unwrap();
//     target_conn
//         .send(axum::extract::ws::Message::Text(json))
//         .await
//         .unwrap();

//     // 3. 存数据库（DAO）
//     im_dao::save_msg(sender_id, msg).await;
//     Ok(())
// }

// // 全局Token内存存储（简化版）
// lazy_static::lazy_static! {
//     pub static ref TOKEN_MAP: HashMap<String, u64> = HashMap::new();
// }
