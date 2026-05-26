mod message_record;
mod task_entry;
mod task_log;
mod task_progress;

/// MySQL tables in dependency order: `task_entry` before `task_log` and `task_progress`.
/// Each entry is `(create_table_ddl, create_index_ddls)`.
pub const MYSQL_TABLES: &[(&str, &[&str])] = &[
    (message_record::MYSQL_DDL, message_record::MYSQL_INDEXES),
    (task_entry::MYSQL_DDL, task_entry::MYSQL_INDEXES),
    (task_log::MYSQL_DDL, task_log::MYSQL_INDEXES),
    (task_progress::MYSQL_DDL, task_progress::MYSQL_INDEXES),
];

/// SQLite tables in dependency order.
pub const SQLITE_TABLES: &[(&str, &[&str])] = &[
    (message_record::SQLITE_DDL, message_record::SQLITE_INDEXES),
    (task_entry::SQLITE_DDL, task_entry::SQLITE_INDEXES),
    (task_log::SQLITE_DDL, task_log::SQLITE_INDEXES),
    (task_progress::SQLITE_DDL, task_progress::SQLITE_INDEXES),
];
