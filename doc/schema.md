# myx-im Architecture & Schema

## ER Diagram

```mermaid
erDiagram
    im_users {
        uuid id PK
        varchar username UK
        text password_hash
        timestamptz created_at
    }

    im_chat_messages {
        bigint id PK
        uuid from_uid FK
        uuid to_uid FK
        text content
        smallint msg_type
        boolean seen
        text client_msg_id UK
        timestamptz created_at
    }

    im_read_cursors {
        uuid user_id PK_FK
        uuid peer_uid PK_FK
        bigint last_read_msg_id
    }

    im_groups {
        uuid id PK
        varchar name
        uuid owner_uid FK
        timestamptz created_at
    }

    im_group_members {
        uuid group_id PK_FK
        uuid user_id PK_FK
        timestamptz joined_at
    }

    im_group_messages {
        bigint id PK
        uuid group_id FK
        uuid from_uid FK
        text content
        smallint msg_type
        text client_msg_id UK
        timestamptz created_at
    }

    im_users ||--o{ im_chat_messages : "from_uid"
    im_users ||--o{ im_chat_messages : "to_uid"
    im_users ||--o{ im_read_cursors : "user_id"
    im_users ||--o{ im_read_cursors : "peer_uid"
    im_users ||--o{ im_groups : "owner"
    im_users ||--o{ im_group_members : "member"
    im_users ||--o{ im_group_messages : "from_uid"
    im_groups ||--o{ im_group_members : "members"
    im_groups ||--o{ im_group_messages : "messages"
```

---

## Private Chat — Full Flow (v1 — `mark_delivered` WS command)

```mermaid
sequenceDiagram
    actor Alice
    actor Bob
    participant Server
    participant DB

    Note over Alice, Server: === Registration ===
    Alice->>Server: POST /api/user/register {username, password}
    Server->>DB: INSERT im_users (bcrypt hash)
    Server-->>Alice: 200 {token}

    Bob->>Server: POST /api/user/register
    Server->>DB: INSERT im_users
    Server-->>Bob: 200 {token}

    Note over Alice, Server: === Login & WS Connect ===
    Alice->>Server: GET /im/ws?token=<jwt>
    Server->>Server: verify JWT
    Server->>DB: SELECT undelivered messages
    Server-->>Alice: Push offline messages (delivered=FALSE)
    Server-->>Alice: WS connected (online_users map)

    Bob->>Server: GET /im/ws?token=<jwt>
    Server-->>Bob: WS connected

    Note over Alice, Bob: === Alice sends message to Bob (Bob online) ===
    Alice->>Server: WS {cmd:"private_chat", data:{to_uid:Bob, content:"hello"}}
    Server->>DB: INSERT im_chat_messages (delivered=FALSE)
    DB-->>Server: msg_id=42
    Server->>Bob: Push PrivatePushMsg {from_uid:Alice, content:"hello", send_time}
    Server-->>Alice: ACK {cmd:"private_chat_ack", data:{msg_id:42, delivered:true}}
    Bob->>Bob: Show "hello" + unread badge if not in chat

    Note over Alice, Bob: === Bob opens Alice's chat ===
    Bob->>Server: WS {cmd:"mark_delivered", data:{peer_uid:Alice}}
    Server->>DB: UPDATE delivered=TRUE WHERE to_uid=Bob AND from_uid=Alice
    Server->>DB: UPSERT im_read_cursors (last_read_msg_id=42)
    Server->>Alice: Push read_receipt {peer_uid:Bob, last_read_msg_id:42}
    Alice->>Alice: ✓ Read

    Note over Alice, Bob: === Alice sends to offline Bob ===
    Alice->>Server: WS {cmd:"private_chat", data:{to_uid:Bob, content:"hi"}}
    Server->>DB: INSERT (delivered=FALSE)
    DB-->>Server: msg_id=43
    Server-->>Alice: ACK {msg_id:43, delivered:false}
    Alice->>Alice: ◷ Sent (offline)

    Note over Bob: Bob reconnects later
    Bob->>Server: GET /im/ws?token=<jwt>
    Server->>DB: SELECT undelivered WHERE to_uid=Bob
    Server-->>Bob: Push [msg_id=43, from_uid=Alice, content:"hi"]
    Bob->>Bob: Show "hi" + unread badge 🔴1

    Bob->>Server: WS {cmd:"mark_delivered", data:{peer_uid:Alice}}
    Server->>DB: UPDATE delivered=TRUE
    Server->>Alice: Push delivery_update {msg_ids:[43]}
    Alice->>Alice: ◷ Sent (offline) → ✓ Delivered
    Server->>Alice: Push read_receipt {last_read_msg_id:43}
    Alice->>Alice: ✓ Read
```

---

## Private Chat — Full Flow (v2 — seen-marking in history endpoint)

> **Change from v1:** The `mark_delivered` WS command is removed. Instead, seen-marking
> happens via `mark_seen_from_peer` (UPDATE RETURNING id) in two places:
> 1. Open chat → `GET /api/message/history` marks unseen and pushes `delivery_update`.
> 2. Receive push while viewing chat → frontend sends `mark_seen` WS command.
> Column renamed `delivered` → `seen`. Frontend states:
> ◷ Sending → ◷ Sent (online/offline both) → ✓ Read (delivery_update only).

```mermaid
sequenceDiagram
    actor Alice
    actor Bob
    participant Server
    participant DB

    Note over Alice, Server: === Registration ===
    Alice->>Server: POST /api/user/register {username, password}
    Server->>DB: INSERT im_users (bcrypt hash)
    Server-->>Alice: 200 {token}

    Bob->>Server: POST /api/user/register
    Server->>DB: INSERT im_users
    Server-->>Bob: 200 {token}

    Note over Alice, Server: === Login & WS Connect ===
    Alice->>Server: GET /im/ws?token=<jwt>
    Server->>Server: verify JWT
    Server->>DB: SELECT unseen messages (seen=FALSE)
    Server-->>Alice: Push offline messages
    Server-->>Alice: WS connected (online_users map)

    Bob->>Server: GET /im/ws?token=<jwt>
    Server-->>Bob: WS connected

    Note over Alice, Bob: === Alice sends to Bob, Bob IS viewing Alice's chat ===
    Alice->>Server: WS {cmd:"private_chat", data:{to_uid:Bob, content:"hello"}}
    Server->>DB: INSERT im_chat_messages (seen=FALSE)
    DB-->>Server: msg_id=42
    Server->>Bob: Push PrivatePushMsg {from_uid:Alice, content:"hello"}
    Server-->>Alice: ACK {msg_id:42, delivered:true}
    Alice->>Alice: ◷ Sent
    Bob->>Bob: Show "hello"
    Bob->>Server: WS {cmd:"mark_seen", data:{to_uid:Alice}}
    Server->>DB: UPDATE seen=TRUE RETURNING id<br/>(mark_seen_from_peer)
    Server->>Alice: Push delivery_update {msg_ids:[42], to_uid:Bob}

    Note over Alice, Bob: === Bob opens Alice's chat (loads history) ===
    Bob->>Server: GET /api/message/history?peer_uid=Alice&token=...
    Server->>DB: UPDATE seen=TRUE RETURNING id<br/>(mark_seen_from_peer)
    Server->>DB: SELECT chat history (ORDER BY id DESC LIMIT 50)
    Server-->>Bob: 200 {history items}
    Server->>Alice: Push delivery_update {msg_ids:[...], to_uid:Bob}

    Note over Alice, Bob: === Alice sends to offline Bob ===
    Alice->>Server: WS {cmd:"private_chat", data:{to_uid:Bob, content:"you there?"}}
    Server->>DB: INSERT (seen=FALSE)
    DB-->>Server: msg_id=43
    Server-->>Alice: ACK {msg_id:43, delivered:false}
    Alice->>Alice: ◷ Sent

    Note over Bob: Bob reconnects later
    Bob->>Server: GET /im/ws?token=<jwt>
    Server->>DB: SELECT unseen WHERE to_uid=Bob (seen=FALSE)
    Server-->>Bob: Push [msg_id=43, from_uid=Alice, content:"you there?"]
    Bob->>Bob: Show "you there?" + unread badge 🔴1

    Note over Bob: Bob opens Alice's chat
    Bob->>Server: GET /api/message/history?peer_uid=Alice&token=...
    Server->>DB: UPDATE seen=TRUE WHERE to_uid=Bob AND from_uid=Alice
    Server-->>Bob: 200 {history items}
    Server->>Alice: Push delivery_update {msg_ids:[43], to_uid:Bob}
    Alice->>Alice: ◷ Sent → ✓ Read
```

---

## Group Chat — Full Flow

> Flow covers: send → ACK → push to members → real-time `mark_group_read` →
> `group_delivery_update` with per-message read counts (sender excluded).

```mermaid
sequenceDiagram
    actor Alice
    actor Bob
    actor Carol
    participant Server
    participant DB

    Note over Alice, Carol: Alice, Bob, Carol in group "Dev Team"
    Alice->>Server: WS {cmd:"group_chat", data:{group_id, content, msg_type, from_name:"Alice"}}
    Server->>DB: is_group_member(Alice) → true
    Server->>DB: INSERT im_group_messages
    DB-->>Server: msg_id=100
    Server-->>Alice: ACK {cmd:"group_chat_ack", seq, data:{msg_id:100, send_time}}
    Alice->>Alice: pendingMsgs[seq] → msgById[100] = DOM (◷ Sent)
    Server->>Bob: Push GroupPushMsg {group_id, from_uid:Alice, from_name:"Alice", content}
    Server->>Carol: Push GroupPushMsg (same)

    Note over Bob, Carol: === Bob is viewing the group, Carol is not ===
    Bob->>Bob: appendGroupMsg (real-time)
    Bob->>Server: WS {cmd:"mark_group_read", data:{group_id}}
    Server->>DB: UPSERT im_group_read_cursors (last_read_msg_id=100)
    Server->>DB: get_group_read_counts(group_id, msg_ids=[100])<br/>→ read:1 (Bob), total:2 (Bob+Carol, sender Alice excluded)
    Server->>Alice: Push group_delivery_update {group_id, msg_statuses:[{msg_id:100, read:1, total:2}]}
    Alice->>Alice: handleGroupDeliveryUpdate → msgById[100] → ✓ Read 1/2
    Carol->>Carol: groupUnreadCounts[group_id]++

    Note over Carol: === Carol opens the group ===
    Carol->>Server: GET /api/group/history?group_id=...&token=...
    Server->>DB: UPSERT im_group_read_cursors (last_read_msg_id=100)
    Server->>DB: get_group_read_counts(group_id, msg_ids=[100])<br/>→ read:2 (Bob+Carol), total:2
    Server-->>Carol: 200 {history items}
    Server->>Alice: Push group_delivery_update {msg_statuses:[{msg_id:100, read:2, total:2}]}
    Alice->>Alice: ✓ Read 1/2 → ✓ Read 2/2
```

### Group Read Status State Machine

```mermaid
stateDiagram-v2
    [*] --> Sending: Alice sends group msg
    Sending --> Sent: group_chat_ack (msg_id mapped)
    Sent --> PartialRead: Bob views group → mark_group_read
    Sent --> PartialRead: Bob opens group history
    PartialRead --> AllRead: Carol views group / opens history
    AllRead --> [*]

    note right of Sending
        ◷ Sending (temp seq)
        pendingMsgs[seq] = {el, msg}
    end note

    note right of Sent
        ◷ Sent
        msgById[real_msg_id] = el
    end note

    note right of PartialRead
        ✓ Read 1/2
        group_delivery_update
        handleGroupDeliveryUpdate()
    end note

    note right of AllRead
        ✓ Read 2/2
    end note
```

### Trigger Matrix

| Trigger                  | Initiated by                | Pushed to                     |
| ------------------------ | --------------------------- | ----------------------------- |
| `group_chat` WS          | Sender                      | Members (push) + Sender (ack) |
| `mark_group_read` WS     | Viewer receives push online | Each sender (for their msgs)  |
| `GET /api/group/history` | Viewer loads/opens group    | Each sender (for their msgs)  |

### Key Data Structures

| Structure               | Direction        | Purpose                                          |
| ----------------------- | ---------------- | ------------------------------------------------ |
| `group_chat`            | Client → Server  | Send group message (includes `from_name`)        |
| `group_push`            | Server → Members | Deliver message to online members (excl. sender) |
| `group_chat_ack`        | Server → Sender  | Return real `msg_id`, sender maps `msgById`      |
| `mark_group_read`       | Client → Server  | Viewer marks group read in real-time             |
| `group_delivery_update` | Server → Sender  | Read count changed: `{msg_id, read, total}`      |

---

## System Architecture

```mermaid
graph TB
    subgraph Client
        Browser["Browser<br/>chat.html"]
        WS["WebSocket<br/>/im/ws"]
    end

    subgraph "myx-im Server (axum)"
        Router["Router<br/>GET / | POST /api/* | WS"]
        Handlers["Handlers<br/>auth, message, group"]
        State["AppState<br/>PgPool + online_users map"]
    end

    subgraph Database
        PG[("PostgreSQL<br/>im_users, im_chat_messages<br/>im_groups, im_group_messages<br/>im_read_cursors")]
    end

    Browser -->|HTTP| Router
    Browser -->|WebSocket| WS
    Router --> Handlers
    Handlers --> State
    State --> PG
    WS --> Handlers
```

---

## Key Design Decisions

| Decision                                    | Rationale                                       |
| ------------------------------------------- | ----------------------------------------------- |
| Private & group messages in separate tables | Cleaner queries, different delivery semantics   |
| `seen` flag on messages                     | Offline sync without extra table                |
| `client_msg_id` UNIQUE                      | Dedup at DB level (ON CONFLICT DO NOTHING)      |
| Composite PK on `im_read_cursors`           | One cursor per (user, peer) pair                |
| `ON DELETE CASCADE` on group children       | Auto-cleanup when group deleted                 |
| UUID everywhere                             | No collision risk, client-side generation       |
| `include_str!("../chat.html")`              | Single binary deployment, no static file server |

---

## UI Layout — Desktop

```mermaid
graph TB
    subgraph App["#app (flex row, height: 100dvh)"]
        direction LR
        subgraph Sidebar["#sidebar (30%%, min 220px, max 380px)"]
            direction TB
            Header[".sidebar-header<br/>avatar + username + logout"]
            Search[".search-wrap<br/>user search input + dropdown"]
            Tabs[".tab-bar<br/>💬 Chats | 👥 Groups"]
            ConvList[".conv-list<br/>flex:1 overflow-y:auto<br/>conversation items"]
        end
        subgraph Main["#main (flex:1, column)"]
            direction TB
            ChatArea["#chatArea (flex:1, column)"]
            NoChat["#noChat - 'Select a conversation'"]
            ChatHeader[".chat-header<br/>back button + avatar + name + status"]
            MsgList[".msg-list<br/>flex:1 overflow-y:auto<br/>message bubbles + load-more"]
            TypingHint[".typing-indicator<br/>'Alice is typing...'"]
            InputBar[".input-bar<br/>textarea + Send button"]
            BottomNav[".bottom-nav<br/>💬 Chats | 👥 Groups | 👤 Me<br/>(mobile only)"]
        end
    end

    Header --> Search
    Search --> Tabs
    Tabs --> ConvList
    ChatHeader --> MsgList
    MsgList --> TypingHint
    TypingHint --> InputBar
    ChatArea --> ChatHeader
    ChatArea --> MsgList
    ChatArea --> TypingHint
    ChatArea --> InputBar
```

---

## UI States — View Transitions

```mermaid
stateDiagram-v2
    [*] --> AuthScreen

    state AuthScreen {
        LoginForm: Register / Login
    }

    state ChatInterface {
        state Sidebar {
            ChatsTab: 💬 Chats tab
            GroupsTab: 👥 Groups tab
        }
        state MainPanel {
            SelectPrompt: "Select a conversation"
            PrivateChat: Private chat view
            GroupChat: Group chat view
        }
    }

    AuthScreen --> ChatInterface: login success + WS connect
    SelectPrompt --> PrivateChat: tap conversation / search result
    SelectPrompt --> GroupChat: tap group
    PrivateChat --> SelectPrompt: close / back
    GroupChat --> SelectPrompt: close / back
    ChatsTab --> GroupsTab: tap 👥 Groups
    GroupsTab --> ChatsTab: tap 💬 Chats
```

---

## UI Components — Message Row

```mermaid
graph LR
    subgraph "My message (.msg-row.mine)"
        direction LR
        MAvatar[".m-avatar<br/>initial"]
        subgraph MRight[" "]
            MBubble[".msg-bubble<br/>bg=primary, white text"]
            MTime[".msg-time<br/>HH:MM"]
            MStatus[".msg-status<br/>✓Delivered / ◷Offline / ◷Sending"]
        end
        MAvatar --> MBubble
        MBubble --> MTime
        MTime --> MStatus
    end

    subgraph "Their message (.msg-row)"
        direction LR
        TAvatar[".m-avatar<br/>initial"]
        subgraph TRight[" "]
            TName["sender name<br/>(group only)"]
            TBubble[".msg-bubble<br/>bg=surface2"]
            TTime[".msg-time"]
        end
        TAvatar --> TName
        TName --> TBubble
        TBubble --> TTime
    end
```

---

## JS State Machine

```mermaid
graph TB
    subgraph State["Global State"]
        me["me: {uid, username, token}"]
        ws["ws: WebSocket | null"]
        activePeer["activePeer: Uuid | null"]
        activeGroup["activeGroup: Uuid | null"]
        activeTab["activeTab: 'chats' | 'groups'"]
        peers["peers: {uid → {username}}"]
        convCache["convCache: ConversationItem[]"]
        groupCache["groupCache: GroupInfo[]"]
        unreadCounts["unreadCounts: {peer_uid → number}"]
        pendingMsgs["pendingMsgs: {seq → {el, msg}}"]
        msgById["msgById: {msg_id → DOM element}"]
    end

    subgraph WS_Events["WebSocket Events"]
        onopen["onopen → loadConversations()"]
        onmessage["onmessage → dispatch"]
        onclose["onclose → ws=null"]
    end

    subgraph Dispatch["onmessage dispatch"]
        Push["PrivatePushMsg<br/>→ handlePush()"]
        GroupPush["GroupPushMsg<br/>→ handleGroupPush()"]
        Ack["private_chat_ack<br/>→ handleAck()"]
        Delivery["delivery_update<br/>→ handleDeliveryUpdate()"]
        Typing["typing<br/>→ handleTyping()"]
    end

    onmessage --> Push
    onmessage --> GroupPush
    onmessage --> Ack
    onmessage --> Delivery
    onmessage --> Typing
```

---

## Message Status State Machine

Own messages transition between three states. Status is displayed below each
message bubble and persisted to `localStorage` for survival across reloads.

```mermaid
stateDiagram-v2
    [*] --> Sending: sendMessage()
    Sending --> Sent: handleAck (online or offline)
    Sent --> Read: delivery_update (mark_seen_from_peer)
    Read --> Read: delivery_update (idempotent)
    Sent --> Read: appendMsg (seen=true)
    Sent --> Sent: appendMsg (seen=false)
```

Each transition explained:

| Transition       | Function                 | Why                                                                  |
| ---------------- | ------------------------ | -------------------------------------------------------------------- |
| `Sending → Sent` | `handleAck()`            | ACK returns, clear "Sending...", set `◷ Sent` (online or offline)    |
| `Sent → Read`    | `handleDeliveryUpdate()` | Received `delivery_update` — peer opened chat or sent `mark_seen`    |
| `Read → Read`    | `handleDeliveryUpdate()` | Idempotent, repeated `delivery_update` is harmless                   |
| `Sent → Read`    | `appendMsg()`            | Loading history: `m.seen=true` (already marked in DB), show `✓ Read` |
| `Sent → Sent`    | `appendMsg()`            | Loading history: `m.seen=false`, keep `◷ Sent`                       |

| State   | Label          | CSS class      | Trigger                                 |
| ------- | -------------- | -------------- | --------------------------------------- |
| Sending | `◷ Sending...` | `.pending`     | Message sent, waiting for ACK           |
| Sent    | `◷ Sent`       | `.undelivered` | Peer offline, or history seen=false     |
| Read    | `✓ Read`       | `.delivered`   | delivery_update only (peer viewed chat) |

**Persistence**: `handleDeliveryUpdate` calls `saveMsgStatus(id, 'read')`.
On history load, `appendMsg` checks `localStorage` first, then falls back to `m.seen`.

---

## WebSocket Task Topology

How `handle_im_websocket` (src/router.rs:87) orchestrates 3 concurrent tasks
communicating via channels. Arrows show data flow.

```mermaid
graph TB
    subgraph "handle_im_websocket (main task)"
        MAIN["while let msg = receiver.next()<br/>→ handle_biz_msg()<br/>→ break on Close"]
    end
    subgraph "Unseen Sync (spawned)"
        SYNC["tokio::spawn<br/>reads undelivered from DB<br/>sends via tx"]
    end
    subgraph "Forwarding (spawned)"
        FWD["while let msg = rx.recv()<br/>→ sender.send(Message::Text)<br/>break on WS write error"]
    end
    subgraph "AppState.online_users"
        MAP["HashMap(Uuid, OnlineUser.tx)"]
    end
    MAIN -- "tx clone" --> SYNC
    MAIN -- "tx clone" --> MAP
    MAIN -- "rx (mpsc)" --> FWD
    MAP -- "send_to_user() → tx.send()" --> FWD
    SYNC -- "tx.send()" --> FWD
    FWD -. "rx dropped → cleanup" .-> MAP
```

### Lifecycle

| Event                     | What happens                                                                             |
| ------------------------- | ---------------------------------------------------------------------------------------- |
| Client connects           | `insert_online_user` stores `tx` in map, spawns sync + fwd tasks                         |
| `send_to_user(uid, msg)`  | Looks up `tx` in map, sends → fwd → WS                                                   |
| Client disconnects        | WS `receiver` returns None, main exits. Fwd's `sender.send()` fails, `break`, drops `rx` |
| Kicked by duplicate login | Old fwd breaks, drops `rx`. New connection's `tx` replaces map entry                     |
| Dead entry cleanup        | Next `send_to_user` → `tx.send()` fails (rx dropped) → `mp.remove(&uid)`                 |
