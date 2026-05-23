use crate::model::{ChatHistoryItem, ConversationItem, User, UserSearchItem};

use sqlx::PgPool;
use uuid::Uuid;
pub async fn save_message(
    pool: &PgPool,
    from_uid: Uuid,
    to_uid: Uuid,
    content: &str,
    msg_type: u8,
) -> anyhow::Result<i64> {
    let res = sqlx::query!(
        r#"INSERT INTO im_chat_messages (from_uid, to_uid, content, msg_type) VALUES ($1, $2, $3, $4) RETURNING id"#,
        from_uid as Uuid,
        to_uid as Uuid,
        content,
        msg_type as i16,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| { tracing::error!("save_message failed: {e}"); e })?;

    Ok(res.id)
}

pub async fn save_user(
    pool: &PgPool,
    user_id: Uuid,
    user_name: String,
    password_hash: String,
) -> anyhow::Result<Uuid> {
    let res = sqlx::query!(
        r#"INSERT INTO im_users (id, username, password_hash) VALUES ($1, $2, $3) RETURNING id"#,
        user_id,
        user_name,
        password_hash,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("save_user failed: {e}");
        e
    })?;

    Ok(res.id)
}

pub async fn find_user_by_username(pool: &PgPool, user_name: &str) -> anyhow::Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"SELECT id, username, password_hash, created_at FROM im_users WHERE username = $1"#,
        user_name,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("find_user_by_username failed: {e}");
        e
    })?;

    Ok(user)
}

pub async fn get_chat_history(
    pool: &PgPool,
    uid_a: Uuid,
    uid_b: Uuid,
    before: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<ChatHistoryItem>> {
    let rows = sqlx::query_as!(
        ChatHistoryItem,
        r#"
        SELECT
            id AS msg_id,
            from_uid,
            to_uid,
            content,
            msg_type,
            EXTRACT(EPOCH FROM created_at)::bigint * 1000 AS "send_time!"
        FROM im_chat_messages
        WHERE ((from_uid = $1 AND to_uid = $2) OR (from_uid = $2 AND to_uid = $1))
          AND id < COALESCE($3::bigint, 9223372036854775807)
        ORDER BY id DESC
        LIMIT $4
        "#,
        uid_a,
        uid_b,
        before,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_chat_history failed: {e}");
        e
    })?;

    Ok(rows)
}

pub async fn get_undelivered_messages(
    pool: &PgPool,
    uid: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<ChatHistoryItem>> {
    let rows = sqlx::query_as!(
        ChatHistoryItem,
        r#"
        SELECT
            id AS msg_id,
            from_uid,
            to_uid,
            content,
            msg_type,
            EXTRACT(EPOCH FROM created_at)::bigint * 1000 AS "send_time!"
        FROM im_chat_messages
        WHERE to_uid = $1 AND delivered = FALSE
        ORDER BY id ASC
        LIMIT $2
        "#,
        uid,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_undelivered_messages failed: {e}");
        e
    })?;

    Ok(rows)
}

pub async fn mark_messages_delivered(pool: &PgPool, msg_ids: &[i64]) -> anyhow::Result<()> {
    if msg_ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE im_chat_messages SET delivered = TRUE WHERE id = ANY($1)")
        .bind(msg_ids)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("mark_messages_delivered failed: {e}");
            e
        })?;
    Ok(())
}

pub async fn get_conversations(pool: &PgPool, uid: Uuid) -> anyhow::Result<Vec<ConversationItem>> {
    let rows = sqlx::query_as!(
        ConversationItem,
        r#"
        SELECT DISTINCT ON (peer_uid)
            peer_uid AS "peer_uid!: Uuid",
            u.username AS peer_name,
            m.content AS last_msg,
            m.msg_type AS last_msg_type,
            EXTRACT(EPOCH FROM m.created_at)::bigint * 1000 AS "last_time!",
            m.id AS last_msg_id
        FROM im_chat_messages m
        CROSS JOIN LATERAL (
            SELECT CASE WHEN m.from_uid = $1 THEN m.to_uid ELSE m.from_uid END AS peer_uid
        ) peer
        JOIN im_users u ON u.id = peer.peer_uid
        WHERE m.from_uid = $1 OR m.to_uid = $1
        ORDER BY peer_uid, m.id DESC
        "#,
        uid,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_conversations failed: {e}");
        e
    })?;

    Ok(rows)
}

pub async fn search_users(
    pool: &PgPool,
    keyword: &str,
    exclude_uid: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<UserSearchItem>> {
    let pattern = format!("%{}%", keyword);
    let rows = sqlx::query_as!(
        UserSearchItem,
        "SELECT id, username FROM im_users WHERE username ILIKE $1 AND id != $2 LIMIT $3",
        pattern,
        exclude_uid,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("search_users failed: {e}");
        e
    })?;

    Ok(rows)
}
