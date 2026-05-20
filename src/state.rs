use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::Utf8Bytes;
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::dao::save_message;

type UserId = u64;

#[derive(Debug, Clone)]
pub struct AppState {
    db_pool: PgPool,
    // We require unique usernames. This tracks which usernames have been taken.
    online_users: Arc<Mutex<HashMap<UserId, OnlineUser>>>,
}

#[derive(Debug, Clone)]
pub struct OnlineUser {
    uid: UserId,
    tx: mpsc::UnboundedSender<Utf8Bytes>,
}
impl AppState {
    pub fn insert_online_user(
        &self,
        uid: u64,
        tx: mpsc::UnboundedSender<Utf8Bytes>,
    ) -> Option<OnlineUser> {
        let mut mp = self.online_users.lock().unwrap();
        let user = OnlineUser { uid, tx };
        mp.insert(uid, user)
    }

    pub fn send_to_user(&self, uid: u64, msg: Utf8Bytes) {
        let Ok(mut mp) = self.online_users.lock() else {
            println!("lock failed");
            return;
        };
        let Some(sender) = mp.get(&uid) else {
            println!("用户 {uid} 不在线，消息丢弃");
            return;
        };

        if let Err(e) = sender.send(msg) {
            mp.remove(&uid);
            println!("用户 {uid} 连接已断开，自动清理: {e}");
        }
    }

    pub async fn save_message(
        &self,
        from_uid: u64,
        to_uid: u64,
        content: &str,
        msg_type: u8,
    ) -> anyhow::Result<i64> {
        save_message(&self.db_pool, from_uid, to_uid, content, msg_type).await
    }
}

impl OnlineUser {
    pub fn send(&self, msg: Utf8Bytes) -> Result<(), mpsc::error::SendError<Utf8Bytes>> {
        self.tx.send(msg)
    }
}

impl AppState {
    fn new(db_pool: PgPool) -> AppState {
        Self {
            db_pool,
            online_users: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub fn init_app_state(db_pool: PgPool) -> AppState {
    AppState::new(db_pool)
}
