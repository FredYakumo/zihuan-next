pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS task_entry (
        id VARCHAR(64) PRIMARY KEY,
        task_type VARCHAR(32) NOT NULL,
        graph_name VARCHAR(256) NOT NULL,
        graph_session_id VARCHAR(128) NOT NULL,
        file_path VARCHAR(512),
        is_workflow_set BOOLEAN NOT NULL DEFAULT FALSE,
        start_time DATETIME NOT NULL,
        is_running BOOLEAN NOT NULL DEFAULT TRUE,
        end_time DATETIME,
        duration_ms BIGINT,
        user_ip VARCHAR(64),
        owner_id VARCHAR(128),
        status VARCHAR(32) NOT NULL,
        error_message TEXT,
        result_summary TEXT,
        can_rerun BOOLEAN NOT NULL DEFAULT FALSE
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS task_entry (
        id TEXT PRIMARY KEY,
        task_type TEXT NOT NULL,
        graph_name TEXT NOT NULL,
        graph_session_id TEXT NOT NULL,
        file_path TEXT,
        is_workflow_set INTEGER NOT NULL DEFAULT 0,
        start_time TEXT NOT NULL,
        is_running INTEGER NOT NULL DEFAULT 1,
        end_time TEXT,
        duration_ms INTEGER,
        user_ip TEXT,
        owner_id TEXT,
        status TEXT NOT NULL,
        error_message TEXT,
        result_summary TEXT,
        can_rerun INTEGER NOT NULL DEFAULT 0
    )";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS ix_task_entry_owner_id ON task_entry (owner_id)",
    "CREATE INDEX IF NOT EXISTS ix_task_entry_status_end_time ON task_entry (status, end_time)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS ix_task_entry_owner_id ON task_entry (owner_id)",
    "CREATE INDEX IF NOT EXISTS ix_task_entry_status_end_time ON task_entry (status, end_time)",
];
