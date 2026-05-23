CREATE TABLE im_users (
    id UUID PRIMARY KEY,
    username VARCHAR(20) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE im_chat_messages (
    id BIGSERIAL PRIMARY KEY,
    from_uid UUID NOT NULL,
    to_uid UUID NOT NULL,
    content TEXT NOT NULL,
    msg_type SMALLINT NOT NULL,
    delivered BOOLEAN NOT NULL DEFAULT FALSE,
    client_msg_id TEXT UNIQUE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE im_read_cursors (
    user_id UUID NOT NULL,
    peer_uid UUID NOT NULL,
    last_read_msg_id BIGINT NOT NULL,
    PRIMARY KEY (user_id, peer_uid)
);
