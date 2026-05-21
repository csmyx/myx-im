use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::Utf8Bytes;
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;
use crate::dao::save_user;
use crate::dao::{find_user_by_username, save_message};
use crate::model::User;

#[derive(Debug, Clone)]
pub struct AppState {
    pg_pool: PgPool,
    online_users: Arc<Mutex<HashMap<Uuid, OnlineUser>>>,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct OnlineUser {
    uid: Uuid,
    tx: mpsc::UnboundedSender<Utf8Bytes>,
}
impl AppState {
    pub fn insert_online_user(
        &self,
        uid: Uuid,
        tx: mpsc::UnboundedSender<Utf8Bytes>,
    ) -> Option<OnlineUser> {
        let mut mp = self.online_users.lock().unwrap();
        let user = OnlineUser { uid, tx };
        mp.insert(uid, user)
    }

    pub fn send_to_user(&self, uid: Uuid, msg: Utf8Bytes) {
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
        from_uid: Uuid,
        to_uid: Uuid,
        content: &str,
        msg_type: u8,
    ) -> anyhow::Result<u64> {
        save_message(&self.pg_pool, from_uid, to_uid, content, msg_type).await
    }

    pub async fn save_user(
        &self,
        user_id: Uuid,
        user_name: String,
        password_hash: String,
    ) -> anyhow::Result<Uuid> {
        save_user(&self.pg_pool, user_id, user_name, password_hash).await
    }

    pub async fn find_user_by_username(&self, user_name: &str) -> anyhow::Result<User> {
        find_user_by_username(&self.pg_pool, user_name).await
    }

    // let user = match state.find_user_by_username(&req.username).await {
}

impl OnlineUser {
    pub fn send(&self, msg: Utf8Bytes) -> Result<(), mpsc::error::SendError<Utf8Bytes>> {
        self.tx.send(msg)
    }
}

impl AppState {
    fn new(pg_pool: PgPool) -> AppState {
        Self {
            pg_pool,
            online_users: Arc::new(Mutex::new(HashMap::new())),
            config: Config::load(),
        }
    }
}

pub fn init_app_state(db_pool: PgPool) -> AppState {
    AppState::new(db_pool)
}
