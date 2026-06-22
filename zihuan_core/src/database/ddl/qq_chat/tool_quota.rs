pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_tool_quota (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    agent_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    tool_name VARCHAR(255) NOT NULL,
    remaining_calls BIGINT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
)";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_agent_service_tool_quota (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    remaining_calls INTEGER NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX idx_qq_chat_agent_service_tool_quota_agent_sender_tool ON qq_chat_agent_service_tool_quota (agent_id, sender_id, tool_name)",
    "CREATE INDEX idx_qq_chat_agent_service_tool_quota_agent_sender ON qq_chat_agent_service_tool_quota (agent_id, sender_id)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_qq_chat_agent_service_tool_quota_agent_sender_tool ON qq_chat_agent_service_tool_quota (agent_id, sender_id, tool_name)",
    "CREATE INDEX IF NOT EXISTS idx_qq_chat_agent_service_tool_quota_agent_sender ON qq_chat_agent_service_tool_quota (agent_id, sender_id)",
];
