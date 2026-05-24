# 1v1 私聊完善 TODO

## 🔴 高优先级

### 1. 规范推送格式（使用 PrivatePushMsg） ✅

- [x] `router.rs` `handle_biz_msg` → `private_chat` 分支：将 `req.content` 裸推送改为序列化 `PrivatePushMsg` JSON
- [x] 推送时附带 `send_time`（毫秒时间戳）

### 2. 消息发送确认（ACK） ✅

- [x] `handle_biz_msg` → `private_chat` 分支：保存成功后通过 `tx` 回一个 ACK
- [x] 如果 `save_message` 失败，回复 error 给发送方
- [x] model 新增 `PrivateChatAck { msg_id, send_time }`

### 3. 聊天历史 API ✅

- [x] `GET /api/message/history?token=<jwt>&peer_uid=<uuid>&before=<msg_id>&limit=<n>`
  - 分页游标：`before` 为上一页最后一条消息 id，首次请求不传
  - `limit` 默认 50，最大 100
- [x] DAO 新增 `get_chat_history(pool, uid_a, uid_b, before, limit)` 方法
- [x] `router.rs` 新增 handler `message_history_handler`

### 4. 离线消息同步 ✅

- [x] `im_chat_messages` 加 `delivered BOOLEAN DEFAULT FALSE` 字段
- [x] DAO 新增 `get_undelivered_messages` / `mark_messages_delivered`
- [x] `handle_im_websocket` 连接建立后执行一次离线消息同步推送

---

## 🟡 中优先级

### 5. 会话列表 API ✅

- [x] `GET /api/conversations?token=<jwt>`
- [x] DAO 新增 `get_conversations` 查询（DISTINCT ON + JOIN）

### 6. 用户搜索 API ✅

- [x] `GET /api/user/search?token=<jwt>&q=<keyword>&limit=<n>`
- [x] DAO 新增 `search_users`（ILIKE 模糊搜索）
  - 按用户名模糊搜索，排除自己
  - 返回 `{ user_id, username }` 列表

### 12. 送达状态追踪 ✅

- [x] `send_to_user` 返回 `bool`（是否在线送达）
- [x] `PrivateChatAck.delivered` 告知发送方对方是否在线
- [x] `mark_delivered` WS 命令：接收方点开对话后标记已送达
- [x] `delivery_update` 推送：通知发送方消息已送达
- [x] 会话列表未读 badge（红色数字角标）
- [x] 消息气泡显示 ✓ Delivered / ◷ Sent (offline) / ◷ Sending...
- [x] 离线消息同步不再自动标记 delivered，改为用户点开对话后触发

---

## 🟢 低优先级

### 7. 消息已读回执 ✅

- [x] 新增表 `im_read_cursors(user_id, peer_uid, last_read_msg_id)`
- [x] DAO `upsert_read_cursor` 写入/更新已读位置
- [x] `mark_delivered` 时同步更新 read cursor
- [x] WS 推送 `read_receipt` 通知发送方对方已读
- [x] 客户端显示 ✓ Read

### 8. 消息去重（幂等） ✅

- [x] `PrivateChatReq` 增加 `client_msg_id: Option<String>` 字段
- [x] `im_chat_messages` 增加 `client_msg_id TEXT UNIQUE` 列
- [x] DAO `save_message` 用 `ON CONFLICT DO NOTHING` + 回查去重

### 9. 正在输入状态 ✅

- [x] WS 命令透传 `{cmd: "typing", data: {to_uid: ...}}`
- [x] 服务端纯转发
- [x] 客户端 debounce 500ms 发送，接收端 3 秒超时隐藏

---

## 🔵 额外完成

### 12. 送达状态追踪 ✅

- [x] `send_to_user` 返回 `bool`（是否在线送达）
- [x] `PrivateChatAck.delivered` 告知发送方对方是否在线
- [x] `mark_delivered` WS 命令：接收方点开对话后标记已送达
- [x] `delivery_update` 推送：通知发送方消息已送达
- [x] 会话列表未读 badge（红色数字角标）
- [x] 消息气泡显示 ✓ Delivered / ◷ Sent (offline) / ◷ Sending...

### 13. Tracing 日志 ✅

- [x] `dao.rs` 全部 8 个函数 error 日志
- [x] `jwt.rs` JWT 签发/验证日志
- [x] `service.rs` 注册/登录/登出全链路日志
- [x] `router.rs` WS 连接/断开/未知命令/序列化失败日志
- [x] `doc/logging.md` 日志参考文档

### 10. 错误处理完善 ✅

- [x] `handle_biz_msg` 中 `save_message` 的错误不再被 `let _` 吞掉
  - 失败时通过 `tx` 回复 `{cmd: "error", seq: <seq>, data: {code: 500, msg: "save failed"}}`

### 11. 真实聊天 UI ✅

- [x] `chat.html` — 真实用户聊天界面（`GET /`）
  - 登录/注册独立界面
  - 左侧会话列表 + 右侧聊天区域（桌面），移动端全屏切换
  - 用户搜索、开始新对话
  - 气泡式消息展示，区分自己和对方
  - 历史消息游标分页加载（"Load earlier messages"）
  - WebSocket 实时推送自动追加
  - 移动端响应式布局（≤700px 切换全屏模式 + 返回按钮）
  - CSS 变量集中管理主题色，方便换肤和后续扩展
- [x] `debug.html` — 调试面板移至 `GET /debug`

---

# 群聊 + 移动端 UI 重构 TODO

## Phase 1: Database & Model

### 14. 群聊数据库表 ✅

- [x] `im_groups(id UUID, name, owner_uid, created_at)`
- [x] `im_group_members(group_id, user_id, joined_at)` — 联合主键
- [x] `im_group_messages(id BIGSERIAL, group_id, from_uid, content, msg_type, client_msg_id?, created_at)`

### 15. 群聊 Model ✅

- [x] `GroupChatReq` / `GroupChatAck` / `GroupPushMsg` — 修复 `from_uid`/`group_id` 为 Uuid
- [x] `CreateGroupReq { token, name }` / `GroupActionReq { token, group_id }`
- [x] `GroupInfo` / `GroupMember` / `GroupHistoryItem`
- [x] `GroupQuery` / `GroupHistoryQuery` — 查询参数

### 16. 群聊 DAO（9 个函数） ✅

- [x] `create_group` — INSERT im_groups + im_group_members
- [x] `join_group` / `leave_group`
- [x] `list_my_groups` — JOIN + GROUP BY
- [x] `list_group_members` — JOIN im_users
- [x] `get_group_history` — 游标分页
- [x] `save_group_message` — 去重（ON CONFLICT client_msg_id）
- [x] `is_group_member` — 权限校验
- [x] `get_group_member_uids` — 推送时获取在线成员

---

## Phase 2: API Endpoints

### 17. REST 接口 ✅

- [x] `POST /api/group/create` — 创建群，自动加入 owner
- [x] `POST /api/group/join` — 加入群
- [x] `POST /api/group/leave` — 退出群
- [x] `GET /api/group/list?token=` — 我的群列表
- [x] `GET /api/group/members?token=&group_id=` — 群成员列表
- [x] `GET /api/group/history?token=&group_id=&before=&limit=` — 群消息历史

---

## Phase 3: WebSocket

### 18. group_chat 命令 ✅

- [x] 校验发送者是群成员
- [x] `save_group_message` → msg_id
- [x] 给所有在线成员（除自己）推送 `GroupPushMsg`
- [x] ACK 发送方 `GroupChatAck { msg_id, send_time, online_count }`

---

## Phase 4: UI — Desktop Tab Navigation

### 19. 会话列表 Tab 切换 ✅

- [x] 侧边栏顶部 `💬 Chats | 👥 Groups` Tab 按钮
- [x] JS `activeTab` 状态，点击切换筛选
- [x] Groups 列表数据来自 `GET /api/group/list`

---

## Phase 5: UI — Group Chat View

### 20. 群聊界面 ✅

- [x] 打开群聊 → 加载群消息历史（`GET /api/group/history`）
- [x] 发送消息 → WS `group_chat` 命令
- [x] 接收推送 → `GroupPushMsg` 解析显示（每条消息前显示发送者名+头像）
- [x] 消息去重使用 `client_msg_id`

---

## Phase 6: UI — Group Management

### 21. 群管理 ✅

- [x] Groups 列表顶部 "+ Create Group" / "Join Group" 按钮
- [x] 创建群弹窗：输入群名 → POST /api/group/create → 刷新列表
- [x] 加入群弹窗：输入群 UUID → POST /api/group/join → 刷新列表
- [x] 弹窗点击背景关闭

---

## Phase 7: UI — Mobile Bottom Nav

### 22. 移动端底部导航 ✅

- [x] 固定底部条：`💬 Chats | 👥 Groups | 👤 Me`
- [x] `Me` 页面：头像、用户名、UID、Logout 按钮
- [x] CSS `safe-area-inset-bottom` 适配刘海屏

---

## 剩余

### 21. 群管理对话框

- [ ] "+" 按钮 → 创建/加入群弹窗
- [ ] 群信息面板：名称 + 成员列表 + 退出按钮

### 23. 账号注销 ✅

- [x] `POST /api/user/delete` — JWT 验证后删除用户及关联数据
- [x] DAO `delete_user` — 按序清理：私聊消息 → 群组 → 群消息 → 已读光标 → 用户记录
- [x] 前端 Me 页面 "Delete Account" 按钮 + 确认弹窗
- [x] `tests/integration_test.rs` `test_delete_account_removes_user_and_data`

---

# 好友功能 TODO

### 24. 好友功能 ✅

- [x] DB: `im_friends(user_id, friend_id, created_at)` — 联合主键, ON DELETE CASCADE
- [x] Model: `AddFriendReq { token, peer_uid }`, `FriendInfo { friend_id, username, created_at }`
- [x] DAO: `add_friend()` (ON CONFLICT DO NOTHING), `list_friends()`
- [x] `POST /api/friend/add` — 添加好友，防自添加
- [x] `GET /api/friend/list` — 好友列表
- [x] 前端: `👤 Friends` tab + 列表 + 搜索 `+ Add` 按钮 + 点击跳转聊天
- [x] Integration tests: `test_friend_add_and_list`, `test_friend_add_self_rejected`
