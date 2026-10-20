pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_context_turn (
    turn_id VARCHAR(64) PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    sender_id VARCHAR(64) NOT NULL,
    sender_name VARCHAR(128) NULL,
    group_name VARCHAR(128) NULL,
    is_group TINYINT(1) NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    user_text MEDIUMTEXT NOT NULL,
    quoted_json MEDIUMTEXT NULL,
    media_json TEXT NULL,
    entries_json MEDIUMTEXT NULL,
    assistant_text MEDIUMTEXT NULL,
    reasoning_text MEDIUMTEXT NULL,
    prompt_tokens BIGINT NULL,
    cached_prompt_tokens BIGINT NULL,
    prompt_cache_miss_tokens BIGINT NULL,
    completion_tokens BIGINT NULL,
    total_tokens BIGINT NULL,
    generation_ms BIGINT NULL,
    output_tokens_per_second DOUBLE NULL,
    continues_previous TINYINT(1) NOT NULL DEFAULT 0,
    finished TINYINT(1) NOT NULL DEFAULT 0
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_context_turn (
    turn_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    sender_name TEXT NULL,
    group_name TEXT NULL,
    is_group INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    user_text TEXT NOT NULL,
    quoted_json TEXT NULL,
    media_json TEXT NULL,
    entries_json TEXT NULL,
    assistant_text TEXT NULL,
    reasoning_text TEXT NULL,
    prompt_tokens INTEGER NULL,
    cached_prompt_tokens INTEGER NULL,
    prompt_cache_miss_tokens INTEGER NULL,
    completion_tokens INTEGER NULL,
    total_tokens INTEGER NULL,
    generation_ms INTEGER NULL,
    output_tokens_per_second REAL NULL,
    continues_previous INTEGER NOT NULL DEFAULT 0,
    finished INTEGER NOT NULL DEFAULT 0
)";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE INDEX idx_qq_chat_context_turn_sender ON qq_chat_context_turn (agent_id, sender_id, created_at)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_qq_chat_context_turn_sender ON qq_chat_context_turn (agent_id, sender_id, created_at)",
];
