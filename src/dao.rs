// use crate::model::{RegisterDTO, User};
// use sqlx::PgPool;

/// 检查用户名是否存在
// pub async fn is_username_exists(pool: &PgPool, username: &str) -> Result<bool, sqlx::Error> {
//     let (exists,) = sqlx::query!(
//         r#"SELECT EXISTS(SELECT 1 FROM "im_users" WHERE username = $1)"#,
//         username
//     )
//     .fetch_one(pool)
//     .await?;

//     Ok(exists.unwrap_or(false))
// }
/*
/// 插入用户（注册）
pub async fn insert_user(
    pool: &PgPool,
    dto: &RegisterDTO,
    hash_pwd: &str,
) -> Result<User, sqlx::Error> {
    let user = sqlx::query_as!(
        User,
        r#"
        INSERT INTO "user" (username, password, nickname)
        VALUES ($1, $2, $3)
        RETURNING id, username, password, nickname, avatar, online, create_at
        "#,
        dto.username,
        hash_pwd,
        dto.nickname
    )
    .fetch_one(pool)
    .await?;

    Ok(user)
}
 */
use sqlx::PgPool;
pub async fn save_message(
    pool: &PgPool,
    from_uid: u64,
    to_uid: u64,
    content: &str,
    msg_type: u8,
) -> anyhow::Result<i64> {
    let id = sqlx::query!(
        r#"
            INSERT INTO im_chat_messages (from_uid, to_uid, content, msg_type)
            VALUES ($1, $2, $3, $4)
            "#,
        from_uid as i64,
        to_uid as i64,
        content,
        msg_type as i16,
    )
    .execute(pool)
    .await?;

    Ok(0)
}
