pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_language_style (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    scope_type VARCHAR(32) NOT NULL,
    group_id VARCHAR(128) NULL,
    style_prompt TEXT NOT NULL,
    sample_count INT NOT NULL,
    learned_by_sender_id VARCHAR(128) NOT NULL,
    learned_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
)";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_language_style (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scope_type TEXT NOT NULL,
    group_id TEXT NULL,
    style_prompt TEXT NOT NULL,
    sample_count INTEGER NOT NULL,
    learned_by_sender_id TEXT NOT NULL,
    learned_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
)";

pub const MYSQL_INDEXES: &[&str] =
    &["CREATE UNIQUE INDEX idx_qq_chat_agent_service_language_style_scope_group ON qq_chat_agent_service_language_style (scope_type, group_id)"];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_qq_chat_agent_service_language_style_scope_group ON qq_chat_agent_service_language_style (scope_type, group_id)",
];
