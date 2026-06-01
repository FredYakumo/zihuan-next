pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_ignore_rule (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    agent_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NULL,
    group_id VARCHAR(255) NULL,
    match_key VARCHAR(512) NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
)";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_ignore_rule (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL,
    sender_id TEXT NULL,
    group_id TEXT NULL,
    match_key TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX idx_qq_chat_agent_ignore_rule_agent_match_key ON qq_chat_agent_ignore_rule (agent_id, match_key)",
    "CREATE INDEX idx_qq_chat_agent_ignore_rule_agent_id ON qq_chat_agent_ignore_rule (agent_id)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_qq_chat_agent_ignore_rule_agent_match_key ON qq_chat_agent_ignore_rule (agent_id, match_key)",
    "CREATE INDEX IF NOT EXISTS idx_qq_chat_agent_ignore_rule_agent_id ON qq_chat_agent_ignore_rule (agent_id)",
];
