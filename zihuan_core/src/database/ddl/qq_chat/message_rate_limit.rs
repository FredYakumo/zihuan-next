pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_message_rate_limit (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    agent_id VARCHAR(128) NOT NULL,
    sender_id VARCHAR(128) NOT NULL,
    group_id VARCHAR(128) NOT NULL DEFAULT '',
    scope_type VARCHAR(32) NOT NULL,
    scope_key VARCHAR(128) NOT NULL,
    window_unit VARCHAR(32) NOT NULL,
    window_size BIGINT NOT NULL DEFAULT 1,
    window_started_at DATETIME NOT NULL,
    used_calls BIGINT NOT NULL DEFAULT 0,
    max_calls BIGINT NULL,
    unlimited TINYINT(1) NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
)";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_message_rate_limit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    group_id TEXT NOT NULL DEFAULT '',
    scope_type TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    window_unit TEXT NOT NULL,
    window_size INTEGER NOT NULL DEFAULT 1,
    window_started_at TEXT NOT NULL,
    used_calls INTEGER NOT NULL DEFAULT 0,
    max_calls INTEGER NULL,
    unlimited INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)";

// The bucket unique index is intentionally NOT created here: its column set changed
// (dropped group_id, added window_size) and `CREATE [UNIQUE] INDEX` cannot replace a
// same-named index on existing deployments. It is created/migrated idempotently in
// `ensure_message_rate_limit_schema_*` after the `window_size` column exists.
pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE INDEX idx_qq_chat_agent_service_message_rate_limit_agent_sender ON qq_chat_agent_service_message_rate_limit (agent_id, sender_id)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_qq_chat_agent_service_message_rate_limit_agent_sender ON qq_chat_agent_service_message_rate_limit (agent_id, sender_id)",
];
