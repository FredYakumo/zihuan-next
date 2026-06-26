pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS media_record (
        media_id VARCHAR(256) PRIMARY KEY,
        source VARCHAR(32) NOT NULL,
        original_source TEXT NOT NULL,
        rustfs_path TEXT NOT NULL,
        name VARCHAR(512),
        description TEXT,
        mime_type VARCHAR(128),
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS media_record (
        media_id TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        original_source TEXT NOT NULL,
        rustfs_path TEXT NOT NULL,
        name TEXT,
        description TEXT,
        mime_type TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )";

pub const MYSQL_INDEXES: &[&str] = &[];

pub const SQLITE_INDEXES: &[&str] = &[];
