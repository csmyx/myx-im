# API Reference

All endpoints are served from a single `myx-im` binary on port 3000.

---

## HTTP Endpoints

### `GET /` — Chat UI
Returns the embedded `chat.html` SPA.

### `GET /debug` — Debug page
Returns `debug.html` (development only).

---

### `POST /api/user/register`

| Field      | Type   | Notes              |
| ---------- | ------ | ------------------ |
| `username` | string | 3-20 chars, unique |
| `password` | string | ≥6 chars           |

**Response:** `Res<{ token }>` — JWT token on success (code 200), 409 if user exists.

---

### `POST /api/user/login`

| Field      | Type   |
| ---------- | ------ |
| `username` | string |
| `password` | string |

**Response:** `Res<{ token }>` — JWT token (code 200), 401 if wrong password.

---

### `POST /api/user/logout`

| Field      | Type   |
| ---------- | ------ |
| `username` | string |
| `password` | string |

Removes the user from the online-users map and closes their WS connection.

---

### `POST /api/user/delete`

| Field   | Type         |
| ------- | ------------ |
| `token` | string (JWT) |

Deletes user + all messages + groups owned + cursors. Also kicks active WS sessions via `kicked` push.

---

### `GET /api/message/history`

| Query param | Type   | Notes                             |
| ----------- | ------ | --------------------------------- |
| `token`     | string | JWT                               |
| `peer_uid`  | UUID   | Chat partner                      |
| `before`    | i64?   | Pagination cursor (msg_id < this) |
| `limit`     | i64?   | Default 50, max 100               |

**Response:** `Res<ChatHistoryItem[]>`

**Side effect:** Marks unseen messages from `peer_uid→caller` as `seen=TRUE`.
If any messages were newly marked, pushes `delivery_update` to the peer via WS.

Each `ChatHistoryItem`:
| Field       | Type          |
| ----------- | ------------- |
| `msg_id`    | i64           |
| `from_uid`  | UUID          |
| `to_uid`    | UUID          |
| `content`   | string        |
| `msg_type`  | i16 (1=text)  |
| `seen`      | bool          |
| `send_time` | i64 (unix ms) |

---

### `GET /api/conversations`

| Query param | Type         |
| ----------- | ------------ |
| `token`     | string (JWT) |

**Response:** `Res<ConversationItem[]>` — one entry per peer, ordered by latest message.

---

### `GET /api/user/search`

| Query param | Type         | Notes              |
| ----------- | ------------ | ------------------ |
| `token`     | string (JWT) |
| `q`         | string       | ILIKE `%q%` match  |
| `limit`     | i64?         | Default 20, max 50 |

**Response:** `Res<UserSearchItem[]>` — excludes self.

---

### `POST /api/group/create`

| Field   | Type         |
| ------- | ------------ |
| `token` | string (JWT) |
| `name`  | string       |

Creates group, owner auto-joined as member.

---

### `POST /api/group/join`

| Field      | Type         |
| ---------- | ------------ |
| `token`    | string (JWT) |
| `group_id` | UUID         |

Idempotent (ON CONFLICT DO NOTHING).

---

### `POST /api/group/leave`

| Field      | Type         |
| ---------- | ------------ |
| `token`    | string (JWT) |
| `group_id` | UUID         |

---

### `GET /api/group/list`

| Query param | Type         |
| ----------- | ------------ |
| `token`     | string (JWT) |

**Response:** `Res<GroupInfo[]>` — groups the user is a member of.

---

### `GET /api/group/members`

| Query param | Type         |
| ----------- | ------------ |
| `token`     | string (JWT) |
| `group_id`  | UUID         |

**Response:** `Res<GroupMember[]>`.

---

### `GET /api/group/history`

| Query param | Type         | Notes               |
| ----------- | ------------ | ------------------- |
| `token`     | string (JWT) |
| `group_id`  | UUID         |
| `before`    | i64?         | Pagination cursor   |
| `limit`     | i64?         | Default 50, max 100 |

**Response:** `Res<GroupHistoryItem[]>`.

---

### `POST /api/friend/add`

| Field      | Type         | Notes                 |
| ---------- | ------------ | --------------------- |
| `token`    | string (JWT) |
| `peer_uid` | UUID         | User to add as friend |

Adds a friend (bidirectional). Idempotent (ON CONFLICT DO NOTHING).
Returns 400 if trying to add self.

---

### `GET /api/friend/list`

| Query param | Type         |
| ----------- | ------------ |
| `token`     | string (JWT) |

**Response:** `Res<FriendInfo[]>` — ordered by username.

Each `FriendInfo`:
| Field        | Type         |
| ------------ | ------------ |
| `friend_id`  | UUID         |
| `username`   | string       |
| `created_at` | timestamptz? |

---

## WebSocket Protocol

Connect: `ws://host:port/im/ws?token=<JWT>`

Auth: JWT verified on upgrade. 401 if invalid.

All messages are JSON: `{cmd, seq, data}`.

### Client → Server (WS Commands)

| cmd                | data                                | Notes                                    |
| ------------------ | ----------------------------------- | ---------------------------------------- |
| `heartbeat`        | —                                   | No-op keepalive                          |
| `typing`           | `{to_uid}`                          | Forwarded to peer                        |
| `mark_seen`        | `{to_uid}`                          | Mark peer's messages as seen (see below) |
| `mark_group_read`  | `{group_id}`                        | Mark group messages as read (see below)  |
| `private_chat`     | `{to_uid, content, msg_type, client_msg_id?, from_name?}` | Send private message     |
| `group_chat`       | `{group_id, content, msg_type, client_msg_id?, from_name?}` | Send group message     |

### Server → Client (WS Pushes)

| cmd                     | data                    | Notes                                                  |
| ----------------------- | ----------------------- | ------------------------------------------------------ |
| `private_push`          | `PrivatePushMsg`        | New private message from peer                          |
| `group_push`            | `GroupPushMsg`          | New group message                                      |
| `private_chat_ack`      | `PrivateChatAck`        | Confirm message saved + delivery status                |
| `group_chat_ack`        | `GroupChatAck`          | Confirm group message saved                            |
| `delivery_update`       | `{msg_ids, to_uid}`     | Private messages now marked seen by peer               |
| `group_delivery_update` | `GroupDeliveryUpdate`   | Group message read counts updated (see below)          |
| `typing`                | `{from_uid}`            | Peer is typing                                         |
| `kicked`                | `{msg}`                 | Session terminated (duplicate login / account deleted) |

---

## `mark_seen` — Mark Peer's Messages as Seen

### Purpose

When the user is **actively viewing** a peer's chat, incoming messages should be
immediately marked as seen — without waiting for the user to close and reopen
the chat.

### Trigger

- **Frontend:** `handlePush()` — when receiving a `private_push` while
  `activePeer === from_uid` and `currentPage === 'chat'`, the frontend sends:
  ```json
  {"cmd":"mark_seen","seq":0,"data":{"to_uid":"<peer_uid>"}}
  ```
- **Also triggered by:** `GET /api/message/history` (open chat / load earlier) —
  the backend marks unseen messages inline in `get_chat_history()`.

### Backend Processing

1. Deserialize `to_uid` (the message sender / peer) from request data
2. `dao::mark_seen_from_peer(pool, peer_uid, current_user_uid)`:
   ```sql
   UPDATE im_chat_messages SET seen = TRUE
   WHERE from_uid = $1 AND to_uid = $2 AND seen = FALSE
   RETURNING id
   ```
3. Push `delivery_update {msg_ids, to_uid: current_user}` to the peer

### Frontend Result

`handleDeliveryUpdate()` receives the `msg_ids`, finds matching DOM elements
via `msgById`, and updates their status to `✓ Read`.

### When NOT Triggered

- User is online but viewing a different chat
- User is on the conversation list
- User sends a reply (reply alone does not imply viewing the chat)
- User receives a push while in a group chat

---

## `mark_group_read` — Mark Group Messages as Read

### Purpose

When the user is **actively viewing** a group chat, incoming `group_push`
messages should immediately update the sender's read count — without waiting
for the user to close and reopen the group.

### Trigger

- **Frontend:** `handleGroupPush()` — when receiving a `group_push` while
  `activeGroup === d.group_id` and `currentPage === 'groupChat'`, the frontend sends:
  ```json
  {"cmd":"mark_group_read","seq":0,"data":{"group_id":"<group_id>"}}
  ```
- **Also triggered by:** `GET /api/group/history` — the backend upserts the
  read cursor and pushes `group_delivery_update` to affected senders.

### Backend Processing

1. Parse `group_id` from request data
2. `dao::mark_group_read_and_get_affected()`:
   - Gets latest `msg_id` in the group
   - Upserts `im_group_read_cursors` for `(user_id, group_id)` to that message
   - Returns all message IDs from other senders (not the viewer)
3. `dao::get_group_read_counts()`: for each message, counts how many OTHER
   members (excluding the sender) have read up to it
4. Groups results by sender UID, pushes `group_delivery_update` to each sender
   with per-message `{msg_id, read, total}` counts

### Frontend Result

`handleGroupDeliveryUpdate()` receives the `msg_statuses`, finds matching DOM
elements via `msgById`, and updates `.msg-status` to `✓ Read N/M`.

---

## `group_delivery_update` — Group Read Count Update

Sent by the server to message senders when another member reads their group
messages (either via history view or real-time `mark_group_read`).

### Data format (`GroupDeliveryUpdate`)

```json
{
  "group_id": "<UUID>",
  "msg_statuses": [
    {"msg_id": 42, "read": 1, "total": 2},
    {"msg_id": 43, "read": 1, "total": 2}
  ],
  "reader_uid": "<UUID>"
}
```

| Field          | Type                     | Notes                                              |
| -------------- | ------------------------ | -------------------------------------------------- |
| `group_id`     | UUID                     | Which group the messages belong to                 |
| `msg_statuses` | `GroupMsgReadStatus[]`   | Per-message `{msg_id, read, total}`                |
| `reader_uid`   | UUID                     | The user who just read (for context, not displayed)|

- `read`: how many OTHER members have read up to this message (sender excluded)
- `total`: total OTHER members in the group (sender excluded)

### `GroupMsgReadStatus`

| Field    | Type | Notes                                        |
| -------- | ---- | -------------------------------------------- |
| `msg_id` | i64  | DB message ID                                |
| `read`   | i64  | Number of other members who have read this   |
| `total`  | i64  | Total other members in the group             |

### Trigger Points

| Trigger                    | Initiated by         | Pushed to         |
| -------------------------- | -------------------- | ----------------- |
| `GET /api/group/history`   | Viewer loads history | Each message sender (for messages from others) |
| `mark_group_read` WS cmd   | Viewer receives push while viewing | Each message sender (for messages from others) |

---

## `group_chat_ack` — Group Message Acknowledgement

Sent by the server to confirm a group message was persisted.

### Data format (`GroupChatAck`)

```json
{
  "msg_id": 42,
  "send_time": 1716600000000,
  "online_count": 2
}
```

| Field          | Type  | Notes                                 |
| -------------- | ----- | ------------------------------------- |
| `msg_id`       | i64   | DB-assigned message ID                |
| `send_time`    | u64   | Unix ms when server processed         |
| `online_count` | usize | Online group members who got the push |

### Frontend Processing

`handleGroupAck()` maps `msgById[msg_id] = pendingMsgs[seq].el`, enabling
`handleGroupDeliveryUpdate()` to later find the DOM element and update the
read status.