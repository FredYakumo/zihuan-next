pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_privilege_auth (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    agent_id VARCHAR(128) NOT NULL,
    sender_id VARCHAR(128) NOT NULL,
    auth_key VARCHAR(255) NOT NULL,
    purpose VARCHAR(128) NOT NULL,
    pending_task_id VARCHAR(64) NULL,
    pending_target_id VARCHAR(128) NULL,
    pending_group_id BIGINT NULL,
    pending_is_group TINYINT(1) NOT NULL DEFAULT 0,
    failed_attempts INT NOT NULL DEFAULT 0,
    expires_at DATETIME NOT NULL,
    elevated_until DATETIME NULL,
    consumed TINYINT(1) NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
)";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_privilege_auth (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    auth_key TEXT NOT NULL,
    purpose TEXT NOT NULL,
    pending_task_id TEXT NULL,
    pending_target_id TEXT NULL,
    pending_group_id INTEGER NULL,
    pending_is_group INTEGER NOT NULL DEFAULT 0,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT NOT NULL,
    elevated_until TEXT NULL,
    consumed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
)";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE INDEX idx_qq_chat_agent_service_privilege_auth_agent_sender ON qq_chat_agent_service_privilege_auth (agent_id, sender_id)",
    "CREATE INDEX idx_qq_chat_agent_service_privilege_auth_expires_at ON qq_chat_agent_service_privilege_auth (expires_at)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_qq_chat_agent_service_privilege_auth_agent_sender ON qq_chat_agent_service_privilege_auth (agent_id, sender_id)",
    "CREATE INDEX IF NOT EXISTS idx_qq_chat_agent_service_privilege_auth_expires_at ON qq_chat_agent_service_privilege_auth (expires_at)",
];
