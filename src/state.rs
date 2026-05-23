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
        if let Some(old) = mp.remove(&uid) {
            let _ = old.tx.send(Utf8Bytes::from_static(
                r#"{"cmd":"kicked","seq":0,"data":{"msg":"logged in elsewhere"}}"#,
            ));
        }
        mp.insert(uid, OnlineUser { tx });
    }

    /// Try to deliver a message to an online user. Returns true if delivered.
    ///
    /// # Lazy dead-entry cleanup
    /// If the forwarding task in handle_im_websocket has exited (WS write side
    /// dead), its `rx` was dropped, causing `sender.send(msg)` to fail. We
    /// catch this and remove the stale entry. This is the ONLY cleanup mechanism
    /// for non-explicit disconnects (browser close, network drop) and kicked
    /// connections (duplicate login).
    pub fn send_to_user(&self, uid: Uuid, msg: Utf8Bytes) -> bool {
        let Ok(mut mp) = self.online_users.lock() else {
            return false;
        };
        let Some(sender) = mp.get(&uid) else {
            tracing::debug!("user {uid} offline, message dropped");
            return false;
        };
        // Lazy dead-entry cleanup: receiver dropped → remove.
        if let Err(e) = sender.send(msg) {
            mp.remove(&uid);
            tracing::debug!("user {uid} disconnected, cleaned up: {e}");
            return false;
        }
        true
    }

    pub fn remove_online_user(&self, uid: Uuid) {
        let Ok(mut mp) = self.online_users.lock() else {
            return;
        };
        mp.remove(&uid);
        tracing::debug!("user {uid} removed from online_users (logout)");
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
