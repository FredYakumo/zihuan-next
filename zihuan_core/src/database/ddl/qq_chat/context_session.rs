pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_context_session (
    agent_id VARCHAR(64) NOT NULL,
    sender_id VARCHAR(64) NOT NULL,
    latest_turn_id VARCHAR(64),
    last_interaction_at VARCHAR(40),
    sender_name VARCHAR(128),
    group_id VARCHAR(64),
    group_name VARCHAR(128),
    is_group BIGINT NOT NULL DEFAULT 0,
    window_version BIGINT NOT NULL DEFAULT 0,
    context_tokens BIGINT NOT NULL DEFAULT 0,
    has_context BIGINT NOT NULL DEFAULT 0,
    window_json LONGTEXT,
    PRIMARY KEY (agent_id, sender_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_context_session (
    agent_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    latest_turn_id TEXT,
    last_interaction_at TEXT,
    sender_name TEXT,
    group_id TEXT,
    group_name TEXT,
    is_group INTEGER NOT NULL DEFAULT 0,
    window_version INTEGER NOT NULL DEFAULT 0,
    context_tokens INTEGER NOT NULL DEFAULT 0,
    has_context INTEGER NOT NULL DEFAULT 0,
    window_json TEXT,
    PRIMARY KEY (agent_id, sender_id)
)";

pub const MYSQL_INDEXES: &[&str] = &["CREATE INDEX idx_context_session_time ON qq_chat_context_session (agent_id, last_interaction_at, sender_id)"];
pub const SQLITE_INDEXES: &[&str] = &["CREATE INDEX IF NOT EXISTS idx_context_session_time ON qq_chat_context_session (agent_id, last_interaction_at, sender_id)"];
