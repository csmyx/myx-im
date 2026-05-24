use crate::model::{
    ChatHistoryItem, ConversationItem, GroupHistoryItem, GroupInfo, GroupMember, User,
    UserSearchItem,
};

use sqlx::PgPool;
use uuid::Uuid;
pub async fn persist_chat_message(
    pool: &PgPool,
    from_uid: Uuid,
    to_uid: Uuid,
    content: &str,
    msg_type: u8,
    client_msg_id: Option<&str>,
) -> anyhow::Result<i64> {
    if let Some(cid) = client_msg_id {
        let res = sqlx::query!(
            r#"INSERT INTO im_chat_messages (from_uid, to_uid, content, msg_type, client_msg_id)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (client_msg_id) DO NOTHING
               RETURNING id"#,
            from_uid as Uuid,
            to_uid as Uuid,
            content,
            msg_type as i16,
            cid,
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("save_message failed: {e}");
            e
        })?;

        if let Some(row) = res {
            return Ok(row.id);
        }
        // Conflict — fetch existing
        let existing = sqlx::query!(
            "SELECT id FROM im_chat_messages WHERE client_msg_id = $1",
            cid,
        )
        .fetch_one(pool)
        .await
        .map_err(|e| {
            tracing::error!("save_message dedup lookup failed: {e}");
            e
        })?;
        tracing::info!("save_message dedup: reused id={}", existing.id);
        return Ok(existing.id);
    }

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
        // Duplicate key is an expected case handled by the service layer (409).
        tracing::warn!("save_user failed: {e}");
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

/// Fetch chat history between two users AND mark unseen messages from uid_b→uid_a as seen.
/// Returns (history_items, newly_seen_msg_ids).
pub async fn get_chat_history(
    pool: &PgPool,
    uid_a: Uuid,
    uid_b: Uuid,
    before: Option<i64>,
    limit: i64,
) -> anyhow::Result<(Vec<ChatHistoryItem>, Vec<i64>)> {
    // Mark unseen messages from peer as seen and collect their IDs
    let seen_rows = sqlx::query!(
        r#"UPDATE im_chat_messages
           SET seen = TRUE
           WHERE from_uid = $1 AND to_uid = $2 AND seen = FALSE
           RETURNING id"#,
        uid_b,
        uid_a,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let seen_ids: Vec<i64> = seen_rows.into_iter().map(|r| r.id).collect();

    let rows = sqlx::query_as!(
        ChatHistoryItem,
        r#"
        SELECT
            id AS msg_id,
            from_uid,
            to_uid,
            content,
            msg_type,
            seen,
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

    Ok((rows, seen_ids))
}

pub async fn get_unseen_messages(
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
            seen,
            EXTRACT(EPOCH FROM created_at)::bigint * 1000 AS "send_time!"
        FROM im_chat_messages
        WHERE to_uid = $1 AND seen = FALSE
        ORDER BY id ASC
        LIMIT $2
        "#,
        uid,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_unseen_messages failed: {e}");
        e
    })?;

    Ok(rows)
}

pub async fn mark_messages_seen(pool: &PgPool, msg_ids: &[i64]) -> anyhow::Result<()> {
    if msg_ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE im_chat_messages SET seen = TRUE WHERE id = ANY($1)")
        .bind(msg_ids)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("mark_messages_seen failed: {e}");
            e
        })?;
    Ok(())
}

pub async fn get_unseen_ids_from_peer(
    pool: &PgPool,
    from_uid: Uuid,
    to_uid: Uuid,
) -> anyhow::Result<Vec<i64>> {
    let rows = sqlx::query!(
        "SELECT id FROM im_chat_messages WHERE from_uid = $1 AND to_uid = $2 AND seen = FALSE ORDER BY id",
        from_uid,
        to_uid,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_unseen_ids_from_peer failed: {e}");
        e
    })?;

    Ok(rows.into_iter().map(|r| r.id).collect())
}
// ===== Group DAO =====

pub async fn create_group(pool: &PgPool, name: &str, owner_uid: Uuid) -> anyhow::Result<GroupInfo> {
    let group_id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO im_groups (id, name, owner_uid) VALUES ($1, $2, $3)",
        group_id,
        name,
        owner_uid,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("create_group failed: {e}");
        e
    })?;

    sqlx::query!(
        "INSERT INTO im_group_members (group_id, user_id) VALUES ($1, $2)",
        group_id,
        owner_uid,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("create_group add owner failed: {e}");
        e
    })?;

    Ok(GroupInfo {
        group_id,
        name: name.to_owned(),
        owner_uid,
        member_count: 1,
        created_at: None,
    })
}

pub async fn join_group(pool: &PgPool, group_id: Uuid, user_id: Uuid) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO im_group_members (group_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        group_id,
        user_id,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("join_group failed: {e}");
        e
    })?;
    Ok(())
}

pub async fn leave_group(pool: &PgPool, group_id: Uuid, user_id: Uuid) -> anyhow::Result<()> {
    sqlx::query!(
        "DELETE FROM im_group_members WHERE group_id = $1 AND user_id = $2",
        group_id,
        user_id,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("leave_group failed: {e}");
        e
    })?;
    Ok(())
}

pub async fn list_my_groups(pool: &PgPool, user_id: Uuid) -> anyhow::Result<Vec<GroupInfo>> {
    let rows = sqlx::query_as!(
        GroupInfo,
        r#"SELECT g.id AS group_id, g.name, g.owner_uid,
                  COUNT(m.user_id)::bigint AS "member_count!",
                  g.created_at
           FROM im_groups g
           JOIN im_group_members m ON m.group_id = g.id
           WHERE g.id IN (SELECT group_id FROM im_group_members WHERE user_id = $1)
           GROUP BY g.id
           ORDER BY g.created_at DESC"#,
        user_id,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("list_my_groups failed: {e}");
        e
    })?;
    Ok(rows)
}

pub async fn list_group_members(pool: &PgPool, group_id: Uuid) -> anyhow::Result<Vec<GroupMember>> {
    let rows = sqlx::query_as!(
        GroupMember,
        r#"SELECT u.id AS user_id, u.username
           FROM im_group_members gm
           JOIN im_users u ON u.id = gm.user_id
           WHERE gm.group_id = $1
           ORDER BY gm.joined_at"#,
        group_id,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("list_group_members failed: {e}");
        e
    })?;
    Ok(rows)
}

pub async fn get_group_history(
    pool: &PgPool,
    group_id: Uuid,
    before: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<GroupHistoryItem>> {
    let rows = sqlx::query_as!(
        GroupHistoryItem,
        r#"
        SELECT
            gm.id AS msg_id,
            gm.group_id,
            gm.from_uid,
            u.username AS from_name,
            gm.content,
            gm.msg_type,
            EXTRACT(EPOCH FROM gm.created_at)::bigint * 1000 AS "send_time!"
        FROM im_group_messages gm
        JOIN im_users u ON u.id = gm.from_uid
        WHERE gm.group_id = $1
          AND gm.id < COALESCE($2::bigint, 9223372036854775807)
        ORDER BY gm.id DESC
        LIMIT $3
        "#,
        group_id,
        before,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_group_history failed: {e}");
        e
    })?;
    Ok(rows)
}

pub async fn persist_group_message(
    pool: &PgPool,
    group_id: Uuid,
    from_uid: Uuid,
    content: &str,
    msg_type: u8,
    client_msg_id: Option<&str>,
) -> anyhow::Result<i64> {
    if let Some(cid) = client_msg_id {
        let res = sqlx::query!(
            r#"INSERT INTO im_group_messages (group_id, from_uid, content, msg_type, client_msg_id)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (client_msg_id) DO NOTHING
               RETURNING id"#,
            group_id,
            from_uid,
            content,
            msg_type as i16,
            cid,
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("save_group_message failed: {e}");
            e
        })?;

        if let Some(row) = res {
            return Ok(row.id);
        }
        let existing = sqlx::query!(
            "SELECT id FROM im_group_messages WHERE client_msg_id = $1",
            cid,
        )
        .fetch_one(pool)
        .await
        .map_err(|e| {
            tracing::error!("save_group_message dedup lookup failed: {e}");
            e
        })?;
        return Ok(existing.id);
    }

    let res = sqlx::query!(
        r#"INSERT INTO im_group_messages (group_id, from_uid, content, msg_type)
           VALUES ($1, $2, $3, $4) RETURNING id"#,
        group_id,
        from_uid,
        content,
        msg_type as i16,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("save_group_message failed: {e}");
        e
    })?;

    Ok(res.id)
}

pub async fn is_group_member(pool: &PgPool, group_id: Uuid, user_id: Uuid) -> anyhow::Result<bool> {
    let row = sqlx::query!(
        "SELECT 1 AS _exists FROM im_group_members WHERE group_id = $1 AND user_id = $2",
        group_id,
        user_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("is_group_member failed: {e}");
        e
    })?;
    Ok(row.is_some())
}

pub async fn get_group_member_uids(pool: &PgPool, group_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
    let rows = sqlx::query!(
        "SELECT user_id FROM im_group_members WHERE group_id = $1",
        group_id,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("get_group_member_uids failed: {e}");
        e
    })?;
    Ok(rows.into_iter().map(|r| r.user_id).collect())
}
pub async fn upsert_read_cursor(
    pool: &PgPool,
    user_id: Uuid,
    peer_uid: Uuid,
    last_read_msg_id: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"INSERT INTO im_read_cursors (user_id, peer_uid, last_read_msg_id)
           VALUES ($1, $2, $3)
           ON CONFLICT (user_id, peer_uid) DO UPDATE SET last_read_msg_id = GREATEST(im_read_cursors.last_read_msg_id, $3)"#,
        user_id,
        peer_uid,
        last_read_msg_id,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("upsert_read_cursor failed: {e}");
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

/// Delete a user and all associated data (messages, groups, memberships, cursors).
pub async fn delete_user(pool: &PgPool, user_id: Uuid) -> anyhow::Result<()> {
    // 1. Delete user's private chat messages
    sqlx::query!(
        "DELETE FROM im_chat_messages WHERE from_uid = $1 OR to_uid = $1",
        user_id,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("delete_user: chat messages failed: {e}");
        e
    })?;

    // 2. Find groups owned by this user
    let owned_groups: Vec<Uuid> =
        sqlx::query!("SELECT id FROM im_groups WHERE owner_uid = $1", user_id,)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                tracing::error!("delete_user: find owned groups failed: {e}");
                e
            })?
            .into_iter()
            .map(|r| r.id)
            .collect();

    // 3. Delete owned groups (cascade deletes group messages + members)
    for gid in &owned_groups {
        sqlx::query!("DELETE FROM im_groups WHERE id = $1", gid)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("delete_user: delete group {gid} failed: {e}");
                e
            })?;
    }

    // 4. Delete user's group messages in groups they don't own
    sqlx::query!("DELETE FROM im_group_messages WHERE from_uid = $1", user_id,)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("delete_user: group messages failed: {e}");
            e
        })?;

    // 5. Delete read cursors
    sqlx::query!(
        "DELETE FROM im_read_cursors WHERE user_id = $1 OR peer_uid = $1",
        user_id,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("delete_user: read cursors failed: {e}");
        e
    })?;

    // 6. Delete user (cascade deletes group_members via ON DELETE CASCADE)
    sqlx::query!("DELETE FROM im_users WHERE id = $1", user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("delete_user: user record failed: {e}");
            e
        })?;

    tracing::info!("user {user_id} and all associated data deleted");
    Ok(())
}
