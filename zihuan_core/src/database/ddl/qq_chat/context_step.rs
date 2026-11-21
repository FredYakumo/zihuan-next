pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_context_step (
    step_id VARCHAR(64) PRIMARY KEY,
    turn_id VARCHAR(64) NOT NULL,
    parent_step_id VARCHAR(64),
    sequence BIGINT NOT NULL,
    kind VARCHAR(32) NOT NULL,
    name VARCHAR(255) NOT NULL,
    status VARCHAR(32) NOT NULL,
    started_at VARCHAR(40) NOT NULL,
    finished_at VARCHAR(40),
    revision BIGINT NOT NULL DEFAULT 1,
    input_json LONGTEXT,
    output_json LONGTEXT,
    error_text TEXT,
    model VARCHAR(255),
    prompt_tokens BIGINT,
    completion_tokens BIGINT,
    total_tokens BIGINT,
    cached_prompt_tokens BIGINT,
    prompt_cache_miss_tokens BIGINT,
    first_token_ms DOUBLE,
    duration_ms DOUBLE,
    output_tokens_per_second DOUBLE,
    FOREIGN KEY (turn_id) REFERENCES qq_chat_context_turn(turn_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_step_id) REFERENCES qq_chat_context_step(step_id) ON DELETE SET NULL,
    UNIQUE (turn_id, sequence)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS qq_chat_context_step (
    step_id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL,
    parent_step_id TEXT,
    sequence INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    input_json TEXT,
    output_json TEXT,
    error_text TEXT,
    model TEXT,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    total_tokens INTEGER,
    cached_prompt_tokens INTEGER,
    prompt_cache_miss_tokens INTEGER,
    first_token_ms REAL,
    duration_ms REAL,
    output_tokens_per_second REAL,
    FOREIGN KEY (turn_id) REFERENCES qq_chat_context_turn(turn_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_step_id) REFERENCES qq_chat_context_step(step_id) ON DELETE SET NULL,
    UNIQUE (turn_id, sequence)
)";

pub const MYSQL_INDEXES: &[&str] =
    &["CREATE INDEX idx_context_step_parent ON qq_chat_context_step (parent_step_id)"];
pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_context_step_parent ON qq_chat_context_step (parent_step_id)",
];
