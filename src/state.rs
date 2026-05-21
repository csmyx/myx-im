use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::Utf8Bytes;
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AppState {
    pub pg_pool: PgPool,
    pub config: Config,
    online_users: Arc<Mutex<HashMap<Uuid, OnlineUser>>>,
}

#[derive(Debug, Clone)]
struct OnlineUser {
    tx: mpsc::UnboundedSender<Utf8Bytes>,
}

impl AppState {
    pub fn insert_online_user(&self, uid: Uuid, tx: mpsc::UnboundedSender<Utf8Bytes>) {
        let mut mp = self.online_users.lock().unwrap();
        mp.insert(uid, OnlineUser { tx });
    }

    pub fn send_to_user(&self, uid: Uuid, msg: Utf8Bytes) {
        let Ok(mut mp) = self.online_users.lock() else {
            return;
        };
        let Some(sender) = mp.get(&uid) else {
            tracing::debug!("user {uid} offline, message dropped");
            return;
        };
        if let Err(e) = sender.send(msg) {
            mp.remove(&uid);
            tracing::debug!("user {uid} disconnected, cleaned up: {e}");
        }
    }
}

impl OnlineUser {
    fn send(&self, msg: Utf8Bytes) -> Result<(), mpsc::error::SendError<Utf8Bytes>> {
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
