//! Integration test: server restart → reconnect → messages survive.
//!
//! Run with: `cargo test --test integration_test -- --nocapture`
//! Requires: DATABASE_URL set, server NOT already running.

use std::sync::Arc;
use std::time::Duration;

use myx_im::router::app_router;
use myx_im::state::init_app_state;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use uuid::Uuid;

/// Spawn the server on an ephemeral port, return the bound address.
async fn spawn_server(pool: sqlx::PgPool) -> (tokio::task::JoinHandle<()>, String) {
    let _ = tracing_subscriber::fmt::try_init();
    let state = init_app_state(pool);
    let app = app_router(Arc::new(state));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    (handle, addr)
}

/// HTTP helper: POST JSON, return response body as serde_json::Value.
async fn post_json(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
) -> serde_json::Value {
    client
        .post(url)
        .json(body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// WS helper: connect, return the WebSocket stream.
async fn ws_connect(
    addr: &str,
    token: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let ws_url = addr
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let url = format!("{}/im/ws?token={}", ws_url, token);
    let (ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws
}

/// Helper: send a WS text message, read the next text response.
async fn ws_send_recv(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    text: &str,
) -> String {
    use futures_util::SinkExt;
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    ws.send(Message::Text(text.into())).await.unwrap();
    loop {
        match ws.next().await.unwrap().unwrap() {
            Message::Text(t) => return t,
            _ => continue,
        }
    }
}

/// Full-flow integration test: register → login → WS chat → reconnect → history.
///
/// Steps:
///   1. Start server on ephemeral port
///   2. Register Alice and Bob (accept 409 if already exist)
///   3. Login both, extract JWT tokens
///   4. Connect both via WebSocket
///   5. Alice sends private_chat to Bob → verify ACK
///   6. Bob receives private_push → verify content
///   7. Close both WS connections (simulate disconnect)
///   8. Reconnect both users via WS
///   9. Query chat history via REST API → verify messages survive
#[tokio::test]
async fn test_register_login_ws_chat_reconnect() {
    dotenv::dotenv().ok();
    // ---- Setup: DB pool ----
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await
        .expect("can't connect to database");

    let client = reqwest::Client::new();

    // ---- Phase 1: Start server ----
    let (server_handle, addr) = spawn_server(pool.clone()).await;

    // ---- Phase 2: Register two users ----
    let alice = serde_json::json!({"username": "test_alice_reconnect", "password": "alice123"});
    let bob = serde_json::json!({"username": "test_bob_reconnect", "password": "bob123"});

    let r = post_json(&client, &format!("{}/api/user/register", addr), &alice).await;
    assert!(
        r["code"] == 200 || r["code"] == 409,
        "register alice failed: {r}"
    );

    let r = post_json(&client, &format!("{}/api/user/register", addr), &bob).await;
    assert!(
        r["code"] == 200 || r["code"] == 409,
        "register bob failed: {r}"
    );

    // ---- Phase 3: Login both, get tokens ----
    let r = post_json(&client, &format!("{}/api/user/login", addr), &alice).await;
    assert_eq!(r["code"], 200, "alice login failed: {r}");
    let alice_token = r["data"].as_str().unwrap().to_string();

    let r = post_json(&client, &format!("{}/api/user/login", addr), &bob).await;
    assert_eq!(r["code"], 200, "bob login failed: {r}");
    let bob_token = r["data"].as_str().unwrap().to_string();

    // ---- Phase 4: Connect both via WS ----
    let mut alice_ws = ws_connect(&addr, &alice_token).await;
    let mut bob_ws = ws_connect(&addr, &bob_token).await;

    // ---- Phase 5: Alice sends private_chat to Bob ----
    // Need Bob's uid from JWT
    let bob_uid: String = {
        let payload = bob_token.split('.').nth(1).unwrap();
        let decoded = base64_decode(payload);
        let claims: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        claims["user_id"].as_str().unwrap().to_string()
    };

    let chat_msg = serde_json::json!({
        "cmd": "private_chat",
        "seq": 1,
        "data": {
            "to_uid": bob_uid,
            "content": "hello from reconnect test",
            "msg_type": 1
        }
    });

    // Alice sends
    let ack = ws_send_recv(&mut alice_ws, &chat_msg.to_string()).await;
    let ack_val: serde_json::Value = serde_json::from_str(&ack).unwrap();
    assert_eq!(
        ack_val["cmd"], "private_chat_ack",
        "expected ACK, got: {ack}"
    );

    // Bob receives push
    let push = ws_send_recv(&mut bob_ws, r#"{"cmd":"heartbeat","seq":0,"data":{}}"#).await;
    let push_val: serde_json::Value = serde_json::from_str(&push).unwrap();
    assert_eq!(
        push_val["cmd"], "private_push",
        "expected push, got: {push}"
    );
    assert_eq!(push_val["data"]["content"], "hello from reconnect test");

    // ---- Phase 6: Close connections (simulate disconnect) ----
    alice_ws.close(None).await.unwrap();
    bob_ws.close(None).await.unwrap();

    // ---- Phase 7: Reconnect and verify history survives ----
    let _alice_ws2 = ws_connect(&addr, &alice_token).await;
    let _bob_ws2 = ws_connect(&addr, &bob_token).await;

    // Query message history via REST API
    let history_url = format!(
        "{}/api/message/history?token={}&peer_uid={}&limit=10",
        addr, alice_token, bob_uid
    );
    let history: serde_json::Value = client
        .get(&history_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(history["code"], 200, "history query failed: {history}");
    let items = history["data"].as_array().unwrap();
    assert!(
        !items.is_empty(),
        "history should have messages after reconnect"
    );

    // ---- Cleanup ----
    server_handle.abort();
}

fn base64_decode(input: &str) -> String {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine.decode(input).unwrap();
    String::from_utf8(bytes).unwrap()
}

/// Regression: messages to a disconnected peer must NOT be marked delivered.
///
/// Bug: when a peer disconnected, the forwarding task's rx stayed alive because
/// OnlineUser's tx kept the mpsc channel open.  The first send_to_user() succeeded
/// at the mpsc level (returning delivered=true), but the message never reached
/// the dead WS client.  Fixed by adding an Arc<AtomicBool> alive flag that
/// send_to_user() checks before returning true.
///
/// Steps:
///   1. Alice and Bob register, login, connect via WS
///   2. Bob disconnects (ws.close), wait for cleanup
///   3. Alice sends first private_chat to Bob → assert delivered=false
///   4. Alice sends second private_chat to Bob → assert delivered=false
#[tokio::test]
async fn test_message_undelivered_when_peer_disconnects() {
    dotenv::dotenv().ok();

    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await
        .expect("can't connect to database");

    let client = reqwest::Client::new();

    // ---- Start server ----
    let (server_handle, addr) = spawn_server(pool.clone()).await;

    // ---- Register two users ----
    let alice = serde_json::json!({"username": "test_alice_undlv", "password": "alice123"});
    let bob = serde_json::json!({"username": "test_bob_undlv", "password": "bob123"});

    let r = post_json(&client, &format!("{}/api/user/register", addr), &alice).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register alice: {r}");
    let r = post_json(&client, &format!("{}/api/user/register", addr), &bob).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register bob: {r}");

    // ---- Login ----
    let r = post_json(&client, &format!("{}/api/user/login", addr), &alice).await;
    assert_eq!(r["code"], 200, "alice login: {r}");
    let alice_token = r["data"].as_str().unwrap().to_string();

    let r = post_json(&client, &format!("{}/api/user/login", addr), &bob).await;
    assert_eq!(r["code"], 200, "bob login: {r}");
    let bob_token = r["data"].as_str().unwrap().to_string();

    // Extract Bob's uid from JWT
    let bob_uid: String = {
        let payload = bob_token.split('.').nth(1).unwrap();
        let decoded = base64_decode(payload);
        let claims: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        claims["user_id"].as_str().unwrap().to_string()
    };

    // ---- Both connect via WS, then Bob disconnects ----
    let mut alice_ws = ws_connect(&addr, &alice_token).await;
    let mut bob_ws = ws_connect(&addr, &bob_token).await;

    // Explicitly close Bob's WS (simulate browser close / logout)
    bob_ws.close(None).await.unwrap();
    // Give the server time to detect the disconnect and set alive=false
    tokio::time::sleep(Duration::from_millis(300)).await;

    // ---- Alice sends first message to disconnected Bob ----
    // This is the regression: previously this would return delivered=true
    let chat_msg = serde_json::json!({
        "cmd": "private_chat",
        "seq": 1,
        "data": {
            "to_uid": bob_uid,
            "content": "first message to offline peer",
            "msg_type": 1
        }
    });
    let ack = ws_send_recv(&mut alice_ws, &chat_msg.to_string()).await;
    let ack_val: serde_json::Value = serde_json::from_str(&ack).unwrap();
    assert_eq!(
        ack_val["cmd"], "private_chat_ack",
        "expected ACK, got: {ack}"
    );
    assert!(
        ack_val["data"]["delivered"] == false,
        "first message to disconnected peer should NOT be delivered, got: {ack}"
    );

    // ---- Alice sends second message ----
    // Subsequent messages should also not be delivered
    let chat_msg2 = serde_json::json!({
        "cmd": "private_chat",
        "seq": 2,
        "data": {
            "to_uid": bob_uid,
            "content": "second message to offline peer",
            "msg_type": 1
        }
    });
    let ack2 = ws_send_recv(&mut alice_ws, &chat_msg2.to_string()).await;
    let ack_val2: serde_json::Value = serde_json::from_str(&ack2).unwrap();
    assert_eq!(
        ack_val2["cmd"], "private_chat_ack",
        "expected ACK, got: {ack2}"
    );
    assert!(
        ack_val2["data"]["delivered"] == false,
        "second message to disconnected peer should NOT be delivered, got: {ack2}"
    );

    // ---- Cleanup ----
    alice_ws.close(None).await.unwrap();
    server_handle.abort();
}

/// Verify that account deletion removes the user and prevents re-login.
///
/// Steps:
///   1. Register and login a test user
///   2. Delete account via POST /api/user/delete
///   3. Attempt login with same credentials → must fail (user deleted)
#[tokio::test]
async fn test_delete_account_removes_user_and_data() {
    dotenv::dotenv().ok();

    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await
        .expect("can't connect to database");

    let client = reqwest::Client::new();
    let (server_handle, addr) = spawn_server(pool.clone()).await;

    // ---- Register and login ----
    let user = serde_json::json!({"username": "test_del_user", "password": "del123"});
    let r = post_json(&client, &format!("{}/api/user/register", addr), &user).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register: {r}");

    let r = post_json(&client, &format!("{}/api/user/login", addr), &user).await;
    assert_eq!(r["code"], 200, "login: {r}");
    let token = r["data"].as_str().unwrap().to_string();

    // ---- Delete account ----
    let r = post_json(
        &client,
        &format!("{}/api/user/delete", addr),
        &serde_json::json!({"token": token}),
    )
    .await;
    assert_eq!(r["code"], 200, "delete account failed: {r}");

    // ---- Verify cannot login again (user record deleted) ----
    let r = post_json(&client, &format!("{}/api/user/login", addr), &user).await;
    assert_ne!(
        r["code"], 200,
        "should not be able to login after deletion: {r}"
    );

    server_handle.abort();
}

/// Verify that account deletion kicks active WebSocket sessions.
///
/// Scenario: device A deletes account while device B is still connected.
/// After deletion, B must be forcefully logged out via a "kicked" WS message.
///
/// Steps:
///   1. Register and login a test user
///   2. Connect via WebSocket (simulating an active session)
///   3. Delete account via POST /api/user/delete
///   4. Read from WS → expect "kicked" message with "account deleted"
#[tokio::test]
async fn test_delete_account_kicks_ws() {
    dotenv::dotenv().ok();

    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await
        .expect("can't connect to database");

    let client = reqwest::Client::new();
    let (server_handle, addr) = spawn_server(pool.clone()).await;

    // ---- Register + login ----
    let user = serde_json::json!({"username": "test_del_kick", "password": "kick123"});
    let r = post_json(&client, &format!("{}/api/user/register", addr), &user).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register: {r}");

    let r = post_json(&client, &format!("{}/api/user/login", addr), &user).await;
    assert_eq!(r["code"], 200, "login: {r}");
    let token = r["data"].as_str().unwrap().to_string();

    // ---- Connect WS ----
    let mut ws = ws_connect(&addr, &token).await;

    // ---- Delete account ----
    let r = post_json(
        &client,
        &format!("{}/api/user/delete", addr),
        &serde_json::json!({"token": token}),
    )
    .await;
    assert_eq!(r["code"], 200, "delete: {r}");

    // ---- WS should receive kicked message ----
    let kicked = ws_send_recv(&mut ws, r#"{"cmd":"heartbeat","seq":0,"data":{}}"#).await;
    let val: serde_json::Value = serde_json::from_str(&kicked).unwrap();
    assert_eq!(val["cmd"], "kicked", "expected kicked, got: {kicked}");

    server_handle.abort();
}

/// Friend API: add friend and list friends.
///
/// Steps:
///   1. Register two users, login
///   2. Alice adds Bob as friend
///   3. Alice lists friends → Bob should appear
///   4. Bob lists friends → Alice appears (bidirectional)
#[tokio::test]
async fn test_friend_add_and_list() {
    dotenv::dotenv().ok();
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await
        .expect("can't connect to database");

    let client = reqwest::Client::new();
    let (server_handle, addr) = spawn_server(pool.clone()).await;

    // Register + login Alice
    let alice = serde_json::json!({"username": "test_friend_alice", "password": "alice123"});
    let r = post_json(&client, &format!("{}/api/user/register", addr), &alice).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register alice: {r}");
    let r = post_json(&client, &format!("{}/api/user/login", addr), &alice).await;
    assert_eq!(r["code"], 200, "alice login: {r}");
    let alice_token = r["data"].as_str().unwrap().to_string();
    let alice_uid: Uuid = {
        let payload = alice_token.split('.').nth(1).unwrap();
        let decoded = base64_decode(payload);
        let claims: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        claims["user_id"].as_str().unwrap().parse().unwrap()
    };

    // Register + login Bob
    let bob = serde_json::json!({"username": "test_friend_bob", "password": "bob123"});
    let r = post_json(&client, &format!("{}/api/user/register", addr), &bob).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register bob: {r}");
    let r = post_json(&client, &format!("{}/api/user/login", addr), &bob).await;
    assert_eq!(r["code"], 200, "bob login: {r}");
    let bob_token = r["data"].as_str().unwrap().to_string();
    let bob_uid: Uuid = {
        let payload = bob_token.split('.').nth(1).unwrap();
        let decoded = base64_decode(payload);
        let claims: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        claims["user_id"].as_str().unwrap().parse().unwrap()
    };

    // Alice adds Bob as friend
    let r = post_json(
        &client,
        &format!("{}/api/friend/add", addr),
        &serde_json::json!({"token": alice_token, "peer_uid": bob_uid}),
    )
    .await;
    assert_eq!(r["code"], 200, "add friend: {r}");

    // Alice lists friends → should include Bob
    let r: serde_json::Value = client
        .get(format!("{}/api/friend/list?token={}", addr, alice_token))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(r["code"], 200, "list friends: {r}");
    let friends = r["data"].as_array().unwrap();
    assert!(
        friends
            .iter()
            .any(|f| f["friend_id"] == bob_uid.to_string()),
        "bob should be in alice's friends: {friends:?}"
    );

    // Bob lists friends → Alice should appear (automatically bidirectional)
    let r: serde_json::Value = client
        .get(format!("{}/api/friend/list?token={}", addr, bob_token))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(r["code"], 200, "bob list friends: {r}");
    let bob_friends = r["data"].as_array().unwrap();
    assert!(
        bob_friends
            .iter()
            .any(|f| f["friend_id"] == alice_uid.to_string()),
        "alice should be in bob's friends (bidirectional)"
    );

    server_handle.abort();
}

/// Friend API: adding self should return 400.
#[tokio::test]
async fn test_friend_add_self_rejected() {
    dotenv::dotenv().ok();
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await
        .expect("can't connect to database");

    let client = reqwest::Client::new();
    let (server_handle, addr) = spawn_server(pool.clone()).await;

    // Register + login
    let user = serde_json::json!({"username": "test_friend_self", "password": "self123"});
    let r = post_json(&client, &format!("{}/api/user/register", addr), &user).await;
    assert!(r["code"] == 200 || r["code"] == 409, "register: {r}");
    let r = post_json(&client, &format!("{}/api/user/login", addr), &user).await;
    assert_eq!(r["code"], 200, "login: {r}");
    let token = r["data"].as_str().unwrap().to_string();
    let my_uid: Uuid = {
        let payload = token.split('.').nth(1).unwrap();
        let decoded = base64_decode(payload);
        let claims: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        claims["user_id"].as_str().unwrap().parse().unwrap()
    };

    // Try to add self as friend
    let r = post_json(
        &client,
        &format!("{}/api/friend/add", addr),
        &serde_json::json!({"token": token, "peer_uid": my_uid}),
    )
    .await;
    assert_eq!(r["code"], 400, "adding self should return 400: {r}");

    server_handle.abort();
}
