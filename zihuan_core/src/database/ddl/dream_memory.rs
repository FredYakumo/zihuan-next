pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS dream_memory (
        id VARCHAR(64) PRIMARY KEY,
        agent_id VARCHAR(128) NOT NULL,
        sender_id VARCHAR(128) NOT NULL,
        created_at DATETIME NOT NULL,
        chat_text_char_count BIGINT NOT NULL,
        memory_content TEXT NOT NULL
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS dream_memory (
        id TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        sender_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        chat_text_char_count INTEGER NOT NULL,
        memory_content TEXT NOT NULL
    )";

pub const MYSQL_INDEXES: &[&str] = &["CREATE INDEX ix_dream_memory_agent_sender_created ON dream_memory (agent_id, sender_id, created_at)"];
pub const SQLITE_INDEXES: &[&str] = &["CREATE INDEX IF NOT EXISTS ix_dream_memory_agent_sender_created ON dream_memory (agent_id, sender_id, created_at)"];
