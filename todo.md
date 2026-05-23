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
