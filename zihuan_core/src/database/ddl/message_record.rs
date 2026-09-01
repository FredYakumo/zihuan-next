pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS message_record (
        id INTEGER PRIMARY KEY AUTO_INCREMENT,
        message_id VARCHAR(64) NOT NULL,
        sender_id VARCHAR(64) NOT NULL,
        sender_name VARCHAR(128) NOT NULL,
        send_time DATETIME NOT NULL,
        group_id VARCHAR(64),
        group_name VARCHAR(128),
        content VARCHAR(2048) NOT NULL,
        at_target_list VARCHAR(512),
        media_json TEXT,
        raw_message_json TEXT
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS message_record (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        message_id TEXT NOT NULL,
        sender_id TEXT NOT NULL,
        sender_name TEXT NOT NULL,
        send_time TEXT NOT NULL,
        group_id TEXT,
        group_name TEXT,
        content TEXT NOT NULL,
        at_target_list TEXT,
        media_json TEXT,
        raw_message_json TEXT
    )";

pub const MYSQL_INDEXES: &[&str] =
    &["CREATE INDEX idx_message_record_message_id ON message_record (message_id)"];
pub const SQLITE_INDEXES: &[&str] =
    &["CREATE INDEX IF NOT EXISTS idx_message_record_message_id ON message_record (message_id)"];
