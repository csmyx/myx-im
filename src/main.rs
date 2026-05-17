// src/main.rs
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

// 在线用户连接管理（线程安全）
type OnlineUsers = Arc<DashMap<String, tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>>>;

// 消息结构体
#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    from: String,
    to: String,
    content: String,
    msg_type: String, // text/image/command
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 监听地址
    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(&addr).await?;
    log::info!("launch myx-im at address: {}", addr);

    let online_users = OnlineUsers::default();

    // 循环接受客户端连接
    while let Ok((stream, _)) = listener.accept().await {
        let users = online_users.clone();
        tokio::spawn(handle_connection(stream, users));
    }

    Ok(())
}

async fn handle_connection(stream: tokio::net::TcpStream, online_users: OnlineUsers) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("Error during the websocket handshake: {}", e);
            return;
        }
    };
    let (mut write, mut read) = ws_stream.split();
    let mut user_id = String::new();

    // 读取客户端消息
    while let Some(msg) = read.next().await {
        let msg = msg.unwrap();
        if msg.is_text() {
            let data = msg.to_text().unwrap();
            let chat_msg: ChatMessage = serde_json::from_str(data).unwrap();

            // 首次连接：绑定用户ID
            if user_id.is_empty() {
                user_id = chat_msg.from.clone();
                online_users.insert(user_id.clone(), write);
                log::info!("user {} connected", user_id);
                continue;
            }

            // 消息路由：转发给目标用户
            if let Some(mut target) = online_users.get_mut(&chat_msg.to) {
                let _ = target.send(msg).await;
            }
        }
    }

    // 断开连接：移除用户
    online_users.remove(&user_id);
    println!("用户 {} 下线", user_id);
}
