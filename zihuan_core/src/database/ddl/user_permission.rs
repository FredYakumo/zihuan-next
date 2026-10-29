pub const MYSQL_DDL: &str = "CREATE TABLE IF NOT EXISTS user_permission (
    agent_id VARCHAR(128) NOT NULL,
    user_id VARCHAR(128) NOT NULL,
    permission_key VARCHAR(160) NOT NULL DEFAULT 'access_level',
    payload LONGTEXT NULL,
    access_level INT UNSIGNED NOT NULL DEFAULT 3,
    elevated_level INT UNSIGNED NULL,
    elevated_until VARCHAR(64) NULL,
    PRIMARY KEY (agent_id, user_id, permission_key)
)";
pub const SQLITE_DDL: &str = "CREATE TABLE IF NOT EXISTS user_permission (
    agent_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    permission_key TEXT NOT NULL DEFAULT 'access_level',
    payload TEXT NULL,
    access_level INTEGER NOT NULL DEFAULT 3 CHECK (access_level >= 0),
    elevated_level INTEGER NULL,
    elevated_until TEXT NULL,
    PRIMARY KEY (agent_id, user_id, permission_key)
)";
