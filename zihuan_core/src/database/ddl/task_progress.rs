pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS task_progress (
        id INTEGER PRIMARY KEY AUTO_INCREMENT,
        task_id VARCHAR(64) NOT NULL,
        seq INTEGER NOT NULL,
        message TEXT NOT NULL,
        CONSTRAINT fk_task_progress_task
            FOREIGN KEY (task_id) REFERENCES task_entry(id)
            ON DELETE CASCADE
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS task_progress (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        task_id TEXT NOT NULL,
        seq INTEGER NOT NULL,
        message TEXT NOT NULL,
        FOREIGN KEY (task_id) REFERENCES task_entry(id) ON DELETE CASCADE
    )";

pub const MYSQL_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS ix_task_progress_task_id_seq ON task_progress (task_id, seq)",
];

pub const SQLITE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS ix_task_progress_task_id_seq ON task_progress (task_id, seq)",
];
