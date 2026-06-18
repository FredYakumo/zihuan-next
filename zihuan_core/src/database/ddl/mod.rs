mod agent_avatar;
mod message_record;
mod qq_chat_agent_service_ignore_rule;
mod qq_chat_agent_service_language_style;
mod qq_chat_agent_service_privilege_auth;
mod qq_chat_agent_service_tool_quota;
mod task_entry;
mod task_log;
mod task_progress;

/// MySQL tables in dependency order: `task_entry` before `task_log` and `task_progress`.
/// Each entry is `(create_table_ddl, create_index_ddls)`.
pub const MYSQL_TABLES: &[(&str, &[&str])] = &[
    (agent_avatar::MYSQL_DDL, agent_avatar::MYSQL_INDEXES),
    (message_record::MYSQL_DDL, message_record::MYSQL_INDEXES),
    (
        qq_chat_agent_service_ignore_rule::MYSQL_DDL,
        qq_chat_agent_service_ignore_rule::MYSQL_INDEXES,
    ),
    (
        qq_chat_agent_service_privilege_auth::MYSQL_DDL,
        qq_chat_agent_service_privilege_auth::MYSQL_INDEXES,
    ),
    (
        qq_chat_agent_service_language_style::MYSQL_DDL,
        qq_chat_agent_service_language_style::MYSQL_INDEXES,
    ),
    (
        qq_chat_agent_service_tool_quota::MYSQL_DDL,
        qq_chat_agent_service_tool_quota::MYSQL_INDEXES,
    ),
    (task_entry::MYSQL_DDL, task_entry::MYSQL_INDEXES),
    (task_log::MYSQL_DDL, task_log::MYSQL_INDEXES),
    (task_progress::MYSQL_DDL, task_progress::MYSQL_INDEXES),
];

/// SQLite tables in dependency order.
pub const SQLITE_TABLES: &[(&str, &[&str])] = &[
    (agent_avatar::SQLITE_DDL, agent_avatar::SQLITE_INDEXES),
    (message_record::SQLITE_DDL, message_record::SQLITE_INDEXES),
    (
        qq_chat_agent_service_ignore_rule::SQLITE_DDL,
        qq_chat_agent_service_ignore_rule::SQLITE_INDEXES,
    ),
    (
        qq_chat_agent_service_privilege_auth::SQLITE_DDL,
        qq_chat_agent_service_privilege_auth::SQLITE_INDEXES,
    ),
    (
        qq_chat_agent_service_language_style::SQLITE_DDL,
        qq_chat_agent_service_language_style::SQLITE_INDEXES,
    ),
    (
        qq_chat_agent_service_tool_quota::SQLITE_DDL,
        qq_chat_agent_service_tool_quota::SQLITE_INDEXES,
    ),
    (task_entry::SQLITE_DDL, task_entry::SQLITE_INDEXES),
    (task_log::SQLITE_DDL, task_log::SQLITE_INDEXES),
    (task_progress::SQLITE_DDL, task_progress::SQLITE_INDEXES),
];
