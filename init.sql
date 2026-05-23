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
    created_at TIMESTAMPTZ DEFAULT NOW()
);
