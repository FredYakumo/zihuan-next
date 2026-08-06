pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS scheduled_task (
        id VARCHAR(64) PRIMARY KEY,
        task_name VARCHAR(128) NOT NULL,
        source_service VARCHAR(128) NOT NULL,
        triggered_by VARCHAR(128),
        start_time DATETIME NOT NULL,
        end_time DATETIME,
        status VARCHAR(32) NOT NULL,
        related_task_ids TEXT,
        info_summary TEXT
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS scheduled_task (
        id TEXT PRIMARY KEY,
        task_name TEXT NOT NULL,
        source_service TEXT NOT NULL,
        triggered_by TEXT,
        start_time TEXT NOT NULL,
        end_time TEXT,
        status TEXT NOT NULL,
        related_task_ids TEXT,
        info_summary TEXT
    )";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE INDEX ix_scheduled_task_status_start_time ON scheduled_task (status, start_time)",
    "CREATE INDEX ix_scheduled_task_source_service ON scheduled_task (source_service)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS ix_scheduled_task_status_start_time ON scheduled_task (status, start_time)",
    "CREATE INDEX IF NOT EXISTS ix_scheduled_task_source_service ON scheduled_task (source_service)",
];
