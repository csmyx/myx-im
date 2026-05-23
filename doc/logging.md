# Tracing Log Reference

All log messages emitted by the application, organized by file and severity.

Run with `RUST_LOG=info` or `RUST_LOG=myx_im=debug` to control verbosity.

---

## `src/dao.rs` — Database Access

All DAO errors are logged at `ERROR` level before propagating the error upward.
The caller (service/router) may log additional context.

| Log message                            | Level | Trigger                               |
| -------------------------------------- | ----- | ------------------------------------- |
| `save_message failed: {e}`             | ERROR | INSERT into `im_chat_messages` failed |
| `save_user failed: {e}`                | ERROR | INSERT into `im_users` failed         |
| `find_user_by_username failed: {e}`    | ERROR | SELECT from `im_users` failed         |
| `get_chat_history failed: {e}`         | ERROR | History query failed                  |
| `get_undelivered_messages failed: {e}` | ERROR | Undelivered message query failed      |
| `mark_messages_delivered failed: {e}`  | ERROR | UPDATE delivered flag failed          |
| `get_conversations failed: {e}`        | ERROR | Conversation list query failed        |
| `search_users failed: {e}`             | ERROR | User search query failed              |

---

## `src/jwt.rs` — Token Creation / Verification

| Log message                                 | Level | Trigger                                                      |
| ------------------------------------------- | ----- | ------------------------------------------------------------ |
| `JWT encode failed for user {user_id}: {e}` | ERROR | Token signing failed (e.g. bad secret)                       |
| `JWT verify failed: {e}`                    | WARN  | Token decode/verify failed (expired, tampered, wrong secret) |

---

## `src/service.rs` — Business Logic

### Registration

| Log message                                             | Level | Trigger                                        |
| ------------------------------------------------------- | ----- | ---------------------------------------------- |
| `bcrypt hash failed for user {username}`                | ERROR | Password hashing crashed (very rare)           |
| `register conflict: username {username} already exists` | INFO  | Duplicate username — returns HTTP 409          |
| `token creation failed for new user {uid}`              | ERROR | JWT generation failed right after registration |
| `user {username} registered (uid={uid})`                | INFO  | Registration succeeded                         |

### Login

| Log message                                        | Level | Trigger                                    |
| -------------------------------------------------- | ----- | ------------------------------------------ |
| `login failed: user {username} not found`          | WARN  | Username does not exist — returns HTTP 401 |
| `login failed: wrong password for user {username}` | WARN  | Password mismatch — returns HTTP 401       |
| `token creation failed for user {username} ({id})` | ERROR | JWT generation failed during login         |
| `user {username} logged in (uid={uid})`            | INFO  | Login succeeded                            |

### Logout

| Log message                                         | Level | Trigger                 |
| --------------------------------------------------- | ----- | ----------------------- |
| `logout failed: user {username} not found`          | WARN  | Username does not exist |
| `logout failed: wrong password for user {username}` | WARN  | Password mismatch       |
| `user {username} logged out (uid={uid})`            | INFO  | Logout succeeded        |

---

## `src/router.rs` — HTTP & WebSocket Handlers

### WebSocket Connection

| Log message                          | Level | Trigger                                     |
| ------------------------------------ | ----- | ------------------------------------------- |
| `WS auth failed: {e}`                | WARN  | WS upgrade rejected — invalid/missing token |
| `handle_im_websocket user={user_id}` | DEBUG | WS connection established                   |
| `WS disconnected: user={user_id}`    | INFO  | WS connection closed (graceful or not)      |

### Undelivered Message Sync

| Log message                            | Level | Trigger                                         |
| -------------------------------------- | ----- | ----------------------------------------------- |
| `get_undelivered_messages failed: {e}` | ERROR | Could not fetch offline messages on connect     |
| `mark_messages_delivered failed: {e}`  | ERROR | Could not mark messages as delivered after push |

### Incoming WS Message Dispatch

| Log message                                     | Level | Trigger                                          |
| ----------------------------------------------- | ----- | ------------------------------------------------ |
| `failed to parse WsMessage: {e}`                | WARN  | Client sent invalid JSON — error frame returned  |
| `failed to parse private_chat request: {e}`     | WARN  | `data` field invalid for `private_chat` cmd      |
| `save_message failed: {e}`                      | ERROR | DB write failed — error frame returned to sender |
| `failed to serialize ACK for msg_id={msg_id}`   | ERROR | JSON serialization of `PrivateChatAck` failed    |
| `failed to serialize push msg to user={to_uid}` | ERROR | JSON serialization of `PrivatePushMsg` failed    |
| `unknown WS command: {cmd} from user={uid}`     | WARN  | Client sent unrecognized `cmd` value             |

### REST API Auth Failures

| Log message                      | Level | Trigger                                 |
| -------------------------------- | ----- | --------------------------------------- |
| `history auth failed: {e}`       | WARN  | Invalid token on `/api/message/history` |
| `conversations auth failed: {e}` | WARN  | Invalid token on `/api/conversations`   |
| `user search auth failed: {e}`   | WARN  | Invalid token on `/api/user/search`     |

### REST API Errors

| Log message                     | Level | Trigger                              |
| ------------------------------- | ----- | ------------------------------------ |
| `get_chat_history failed: {e}`  | ERROR | History query returned DB error      |
| `get_conversations failed: {e}` | ERROR | Conversation query returned DB error |
| `search_users failed: {e}`      | ERROR | Search query returned DB error       |

---

## `src/state.rs` — In-Memory Online Users

| Log message                                     | Level | Trigger                                         |
| ----------------------------------------------- | ----- | ----------------------------------------------- |
| `user {uid} offline, message dropped`           | DEBUG | `send_to_user` called but user not online       |
| `user {uid} disconnected, cleaned up`           | DEBUG | mpsc send failed — user removed from online map |
| `user {uid} removed from online_users (logout)` | DEBUG | Explicit logout removed user from online map    |

---

## Debugging Workflow

### Scenario: User can't log in

```
1. Check for "login failed: user {x} not found"  →  username doesn't exist
2. Check for "login failed: wrong password for user {x}"  →  wrong password
3. Check for "find_user_by_username failed"  →  DB connection issue
4. Check for "JWT verify failed" on subsequent requests  →  token expired or tampered
```

### Scenario: Messages not being delivered

```
1. "user {uid} offline, message dropped"  →  recipient not connected (normal)
2. "get_undelivered_messages failed"  →  offline sync broken on reconnect
3. "failed to serialize push msg"  →  data corruption, check message content
4. "save_message failed"  →  DB write failure, check Postgres
```

### Scenario: Client sees error frames

```
1. "failed to parse WsMessage"  →  client sent bad JSON
2. "failed to parse private_chat request"  →  data field missing to_uid/content/etc.
3. "save_message failed" followed by error frame  →  DB is down
4. "unknown WS command"  →  client sent unsupported cmd
```

### Log Level Quick Reference

```
RUST_LOG=myx_im=info    # connections, logins, logouts (production)
RUST_LOG=myx_im=debug   # + WS connect, online/offline transitions
RUST_LOG=myx_im=trace   # everything including sqlx query logs
```
