pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS role_avatar (
        id VARCHAR(64) PRIMARY KEY,
        role_id VARCHAR(64) NOT NULL,
        file_name VARCHAR(256),
        mime_type VARCHAR(64) NOT NULL,
        image_data LONGBLOB NOT NULL,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS role_avatar (
        id TEXT PRIMARY KEY,
        role_id TEXT NOT NULL,
        file_name TEXT,
        mime_type TEXT NOT NULL,
        image_data BLOB NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )";

pub const MYSQL_INDEXES: &[&str] =
    &["CREATE INDEX ix_role_avatar_role_id ON role_avatar (role_id)"];

pub const SQLITE_INDEXES: &[&str] =
    &["CREATE INDEX IF NOT EXISTS ix_role_avatar_role_id ON role_avatar (role_id)"];
