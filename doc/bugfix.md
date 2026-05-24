# Bug Fix Log

## Backend

### 1. First message to disconnected peer marked delivered (2026-05-24)

**Symptom**: When peer disconnects, the first message sent to them is incorrectly marked as "delivered". Subsequent messages correctly show as not delivered.

**Root cause**: When a WebSocket connection drops, `handle_im_websocket`'s main loop exits, but the forwarding task (`tokio::spawn`) is still alive waiting on `rx.recv()`. The mpsc channel's `tx` (stored in `OnlineUser`) is still valid because `rx` hasn't been dropped yet. The first `send_to_user()` succeeds at the mpsc level, returning `true`, so the ACK says "delivered". The message then reaches the forwarding task, which tries to write to the dead WS sender, fails, breaks the loop, and drops `rx`. Only after that do subsequent `tx.send()` calls fail.

**Fix**: Added `Arc<AtomicBool>` alive flag to `OnlineUser` struct. The forwarding task sets it `false` on exit. The WS main loop also sets it `false` after the receiver loop exits (defense in depth). `send_to_user()` checks the alive flag before returning `true` — dead connections are caught immediately without waiting for mpsc send to fail.

**Files changed**:
- `src/state.rs` — `OnlineUser` gains `alive: Arc<AtomicBool>`, `insert_online_user` signature updated, `send_to_user` checks alive
- `src/router.rs` — `handle_im_websocket` creates alive flag, forwarding task + main loop set it false on exit
- `src/dao.rs` — lowered `save_user` log from `ERROR` to `WARN` (duplicate key is expected, handled by service layer as 409)
- `tests/integration_test.rs` — regression test `test_message_undelivered_when_peer_disconnects`

### 2. Account deletion didn't kick active WebSocket sessions (2026-05-24)

**Symptom**: After deleting an account, other devices logged into the same account stayed connected and showed the chat UI.

**Root cause**: `service::delete_user` returned `(StatusCode, Json<...>)` with no way for the route handler to know the deleted user's ID. The route handler couldn't send a kick message to active sessions.

**Fix**:
- `service::delete_user` now returns `Result<Uuid, ...>` — exposes the deleted user's ID
- Route handler sends `{"cmd":"kicked","data":{"msg":"account deleted"}}` via `state.send_to_user()` after successful deletion
- Integration test `test_delete_account_kicks_ws` verifies the kicked message arrives

**Files changed**: `src/service.rs`, `src/router.rs`, `tests/integration_test.rs`

### 3. Messages never marked as seen during active chat (2026-05-24)

**Symptom**: When Bob was viewing Alice's chat and received new messages via WS push,
the messages stayed `seen=FALSE` in the DB. Alice never saw ✓ Read unless Bob
closed and reopened the chat. The `mark_delivered` WS command was the old
mechanism but was removed during the `delivered→seen` refactor.

**Root cause**: Seen-marking only happened in `GET /api/message/history` (open chat),
which doesn't fire during an active chat session. The subsequent attempt to fix
this by marking seen on `private_chat` (reply) was too aggressive — it marked
messages as seen even when Bob wasn't viewing Alice's chat.

**Fix**:
- Removed `mark_seen_from_peer` from `private_chat` handler (replying ≠ viewing)
- Added lightweight `mark_seen` WS command in `handle_biz_msg`
- Frontend `handlePush()` sends `mark_seen` when `activePeer === from_uid` and
  `currentPage === 'chat'` — only when actually viewing the chat
- Merged `get_unseen_ids_from_peer` + `mark_messages_seen` into single
  `mark_seen_from_peer(pool, from_uid, to_uid) → Vec<i64>` (UPDATE RETURNING id)
- Added `seen: bool` to `ChatHistoryItem` for frontend to display read/unread in history
- Frontend shows three states: ◷ Sending → ◷ Sent → ✓ Read (every message has a mark)
- Column renamed `delivered` → `seen` in DB, DAO, and docs

**Files changed**: `src/dao.rs`, `src/router.rs`, `src/model.rs`, `chat.html`,
`init.sql`, `doc/schema.md`, `doc/api.md`

---

## Frontend

### 2. appendChild crash: loadMore destroyed by innerHTML (2026-05-24)

**Symptom**: Clicking a chat user throws `TypeError: Failed to execute 'appendChild' on 'Node': parameter 1 is not of type 'Node'`.

**Root cause**: `openChat()` calls `$('msgList').innerHTML = ''` which removes `loadMore` from the DOM, then `$('msgList').appendChild($('loadMore'))` fails because `$('loadMore')` returns `null`.

**Fix**: Replaced `innerHTML = ''` + `appendChild(loadMore)` with `replaceChildren(loadMore)`. Applied to `openChat`, `renderMessages`, `openGroupChat`, `renderGroupMessages`.

---

### 3. Page nav: inline display:none blocks CSS .page.active (2026-05-24)

**Symptom**: Bottom nav buttons (Chats/Groups/Me) had no response — pages wouldn't switch.

**Root cause**: Page divs had `style="display:none"` inline, which overrides the CSS rule `.page.active { display: flex }`.

**Fix**: Removed `style="display:none"` from all page divs. CSS `.page { display: none }` already hides them.

---

### 4. Emoji rendering as raw Unicode escapes (2026-05-24)

**Symptom**: Bottom nav showed `\U0001f4ac` instead of 💬.

**Root cause**: Python raw string used to generate the HTML doesn't process `\U` escapes.

**Fix**: Replaced all `\U...` sequences with actual emoji characters.

---

### 5. crypto.randomUUID() not available on mobile (2026-05-24)

**Symptom**: Send button no response on phone. `TypeError: crypto.randomUUID is not a function`.

**Root cause**: `crypto.randomUUID()` requires Chrome 92+ / Safari 15.4+.

**Fix**: Added `uuid()` polyfill using `Math.random()` fallback.

---

### 6. me.token null crash on reconnect after logout (2026-05-24)

**Symptom**: `Cannot read properties of null (reading 'token')` in `connectWS`.

**Root cause**: After `doLogout()` sets `me = null`, the WS `onclose` reconnect timer calls `connectWS()`.

**Fix**: Added `if(!me)return` guard in `connectWS()` and reconnect timeout.

---

### 7. Mobile scrolling broken (2026-05-24)

**Symptom**: Chat message list couldn't be scrolled on mobile.

**Root cause**: `100vh` overrode `100dvh` (iOS dynamic viewport). Missing `-webkit-overflow-scrolling: touch`.

**Fix**: Swapped `vh`/`dvh` order. Added `-webkit-overflow-scrolling: touch`. Added `min-height: 0` to `#app`.

---

### 8. Peer avatar/name showing UUID instead of username (2026-05-24)

**Symptom**: Sometimes the chat header avatar and message bubbles showed raw UUID fragments instead of the peer's username.

**Root cause**: `PrivatePushMsg` had no `from_name` field. When a push arrived before `loadConversations`, `handlePush` created a `peers` entry with `username: '?'` placeholder. Then `loadConversations` skipped the update because the entry already existed. The message bubble fallback `m.from_uid.substring(0, 8)` displayed raw UUID.

**Fix**:
- Added `from_name: String` to `PrivatePushMsg` model
- Backend looks up sender username from DB when constructing push (private_chat + sync undelivered)
- Frontend `handlePush` uses `d.from_name` directly instead of placeholder
- `loadConversations` now always overwrites `peers` entries (not just on first creation)
- `openChat` overwrites `peers` entry if username is still `'?'`

**Files changed**: `src/model.rs`, `src/router.rs`, `chat.html`
