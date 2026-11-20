//! Permission groups and entries.
//!
//! Permissions are stored in the system database so every service shares the same view. A group
//! is selected per RoleService; entries grant or deny a category to one user entity. Categories
//! are dot-separated paths and support a trailing `*` wildcard, so `command.new` is matched by
//! `command.new`, `command.*` and `*`, most specific first. An absent entry denies.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{Error, Result};
use crate::system_database::{with_database, SystemDatabase};

pub const DEFAULT_GROUP: &str = "default";

const MYSQL_GROUP_DDL: &str = "CREATE TABLE IF NOT EXISTS zihuan_settings.permission_group (
    name VARCHAR(128) NOT NULL,
    created_at VARCHAR(40) NOT NULL,
    updated_at VARCHAR(40) NOT NULL,
    PRIMARY KEY (name)
)";

const SQLITE_GROUP_DDL: &str = "CREATE TABLE IF NOT EXISTS zihuan_settings.permission_group (
    name TEXT NOT NULL PRIMARY KEY,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
)";

const MYSQL_ENTRY_DDL: &str = "CREATE TABLE IF NOT EXISTS zihuan_settings.permission_entry (
    group_name VARCHAR(128) NOT NULL,
    entity_id VARCHAR(128) NOT NULL,
    entity_type VARCHAR(32) NOT NULL,
    category VARCHAR(255) NOT NULL,
    value INT NOT NULL DEFAULT 0,
    created_at VARCHAR(40) NOT NULL,
    updated_at VARCHAR(40) NOT NULL,
    PRIMARY KEY (group_name, entity_id, entity_type, category)
)";

const SQLITE_ENTRY_DDL: &str = "CREATE TABLE IF NOT EXISTS zihuan_settings.permission_entry (
    group_name TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    category TEXT NOT NULL,
    value INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (group_name, entity_id, entity_type, category)
)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    QqId,
    Wxid,
}

impl EntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QqId => "qq_id",
            Self::Wxid => "wxid",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "qq_id" => Ok(Self::QqId),
            "wxid" => Ok(Self::Wxid),
            other => Err(crate::validation_error!("不支持的实体类型: {other}")),
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGroup {
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub entry_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub group_name: String,
    pub entity_id: String,
    pub entity_type: EntityType,
    pub category: String,
    pub value: i32,
    pub created_at: String,
    pub updated_at: String,
}

pub enum CreateGroupResult {
    Created(PermissionGroup),
    Existing(PermissionGroup),
}

pub enum CreateEntryResult {
    Created(PermissionEntry),
    Conflict(PermissionEntry),
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn default_group(mysql: bool) -> &'static str {
    if mysql {
        "INSERT IGNORE INTO zihuan_settings.permission_group (name, created_at, updated_at) VALUES (?, ?, ?)"
    } else {
        "INSERT INTO zihuan_settings.permission_group (name, created_at, updated_at) VALUES (?, ?, ?) ON CONFLICT(name) DO NOTHING"
    }
}

/// Creates the permission tables and makes sure the default group exists.
pub async fn prepare(database: &SystemDatabase) -> Result<()> {
    let (group_ddl, entry_ddl) = if database.mysql {
        (MYSQL_GROUP_DDL, MYSQL_ENTRY_DDL)
    } else {
        (SQLITE_GROUP_DDL, SQLITE_ENTRY_DDL)
    };
    sqlx::query(group_ddl).execute(&database.pool).await?;
    sqlx::query(entry_ddl).execute(&database.pool).await?;
    let now = now_rfc3339();
    sqlx::query(default_group(database.mysql))
        .bind(DEFAULT_GROUP)
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await?;
    Ok(())
}

pub fn validate_group_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::validation_error!("权限组名不能为空"));
    }
    if name.chars().count() > 128 {
        return Err(crate::validation_error!("权限组名不能超过 128 个字符"));
    }
    if name.contains('/') {
        return Err(crate::validation_error!("权限组名不能包含 /"));
    }
    Ok(name.to_string())
}

fn normalize_group(name: &str) -> Result<String> {
    if name.trim().is_empty() {
        return Ok(DEFAULT_GROUP.to_string());
    }
    validate_group_name(name)
}

pub fn validate_category(category: &str) -> Result<String> {
    let category = category.trim();
    if category.is_empty() {
        return Err(crate::validation_error!("权限类别不能为空"));
    }
    if category.chars().count() > 255 {
        return Err(crate::validation_error!("权限类别不能超过 255 个字符"));
    }
    let segments: Vec<&str> = category.split('.').collect();
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            return Err(crate::validation_error!("权限类别不能包含空段: {category}"));
        }
        if *segment == "*" && index + 1 != segments.len() {
            return Err(crate::validation_error!("通配符 * 只能作为最后一段: {category}"));
        }
        if segment.contains(char::is_whitespace) {
            return Err(crate::validation_error!("权限类别不能包含空白: {category}"));
        }
    }
    Ok(category.to_string())
}

pub fn list_groups(search: Option<&str>) -> Result<Vec<PermissionGroup>> {
    let search = search.map(str::trim).filter(|value| !value.is_empty());
    run_database(async move |database| {
        let mut sql = String::from(
            "SELECT g.name, g.created_at, g.updated_at,
                (SELECT COUNT(*) FROM zihuan_settings.permission_entry e WHERE e.group_name = g.name) AS entry_count
             FROM zihuan_settings.permission_group g",
        );
        if search.is_some() {
            sql.push_str(" WHERE g.name LIKE ?");
        }
        sql.push_str(" ORDER BY g.name");
        let mut query = sqlx::query(&sql);
        if let Some(search) = search {
            query = query.bind(format!("%{search}%"));
        }
        let rows = query.fetch_all(&database.pool).await?;
        rows.iter().map(group_from_row).collect()
    })
}

pub fn group_exists(name: &str) -> Result<bool> {
    let name = normalize_group(name)?;
    run_database(async move |database| {
        Ok(sqlx::query("SELECT 1 FROM zihuan_settings.permission_group WHERE name = ?")
            .bind(&name)
            .fetch_optional(&database.pool)
            .await?
            .is_some())
    })
}

pub fn create_group(name: &str) -> Result<CreateGroupResult> {
    let name = validate_group_name(name)?;
    run_database(async move |database| {
        if let Some(group) = fetch_group(&database, &name).await? {
            return Ok(CreateGroupResult::Existing(group));
        }
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO zihuan_settings.permission_group (name, created_at, updated_at) VALUES (?, ?, ?)",
        )
        .bind(&name)
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await?;
        let group = fetch_group(&database, &name).await?.ok_or(Error::NotFound)?;
        Ok(CreateGroupResult::Created(group))
    })
}

pub fn list_entries(group_name: &str) -> Result<Vec<PermissionEntry>> {
    let group_name = normalize_group(group_name)?;
    run_database(async move |database| {
        let rows = sqlx::query(
            "SELECT group_name, entity_id, entity_type, category, value, created_at, updated_at
             FROM zihuan_settings.permission_entry WHERE group_name = ?
             ORDER BY entity_type, entity_id, category",
        )
        .bind(&group_name)
        .fetch_all(&database.pool)
        .await?;
        rows.iter().map(entry_from_row).collect()
    })
}

pub fn create_entry(
    group_name: &str,
    entity_id: &str,
    entity_type: EntityType,
    category: &str,
    value: i32,
) -> Result<CreateEntryResult> {
    let group_name = normalize_group(group_name)?;
    let entity_id = validate_entity_id(entity_id)?;
    let category = validate_category(category)?;
    run_database(async move |database| {
        if let Some(entry) =
            fetch_entry(&database, &group_name, &entity_id, entity_type, &category).await?
        {
            return Ok(CreateEntryResult::Conflict(entry));
        }
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO zihuan_settings.permission_entry
                (group_name, entity_id, entity_type, category, value, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&group_name)
        .bind(&entity_id)
        .bind(entity_type.as_str())
        .bind(&category)
        .bind(value)
        .bind(&now)
        .bind(&now)
        .execute(&database.pool)
        .await?;
        fetch_entry(&database, &group_name, &entity_id, entity_type, &category)
            .await?
            .ok_or(Error::NotFound)
            .map(CreateEntryResult::Created)
    })
}

pub fn update_entry(
    group_name: &str,
    entity_id: &str,
    entity_type: EntityType,
    category: &str,
    value: i32,
) -> Result<Option<PermissionEntry>> {
    let group_name = normalize_group(group_name)?;
    let entity_id = validate_entity_id(entity_id)?;
    let category = validate_category(category)?;
    run_database(async move |database| {
        let now = now_rfc3339();
        sqlx::query(
            "UPDATE zihuan_settings.permission_entry SET value = ?, updated_at = ?
             WHERE group_name = ? AND entity_id = ? AND entity_type = ? AND category = ?",
        )
        .bind(value)
        .bind(&now)
        .bind(&group_name)
        .bind(&entity_id)
        .bind(entity_type.as_str())
        .bind(&category)
        .execute(&database.pool)
        .await?;
        fetch_entry(&database, &group_name, &entity_id, entity_type, &category).await
    })
}

pub fn delete_entry(
    group_name: &str,
    entity_id: &str,
    entity_type: EntityType,
    category: &str,
) -> Result<bool> {
    let group_name = normalize_group(group_name)?;
    let entity_id = validate_entity_id(entity_id)?;
    let category = validate_category(category)?;
    run_database(async move |database| {
        let result = sqlx::query(
            "DELETE FROM zihuan_settings.permission_entry
             WHERE group_name = ? AND entity_id = ? AND entity_type = ? AND category = ?",
        )
        .bind(&group_name)
        .bind(&entity_id)
        .bind(entity_type.as_str())
        .bind(&category)
        .execute(&database.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    })
}

/// Returns whether `category` is allowed for the entity in `group_name`.
///
/// The most specific entry wins: the exact category, then each wildcarded ancestor, then `*`.
/// A missing entry is a denial.
pub fn check_permission(
    group_name: &str,
    entity_id: &str,
    entity_type: EntityType,
    category: &str,
) -> Result<bool> {
    let group_name = normalize_group(group_name)?;
    let entity_id = entity_id.trim();
    if entity_id.is_empty() {
        return Ok(false);
    }
    let category = validate_category(category)?;
    let candidates = category_candidates(&category);
    run_database(async move |database| {
        let rows = sqlx::query(
            "SELECT category, value FROM zihuan_settings.permission_entry
             WHERE group_name = ? AND entity_id = ? AND entity_type = ?",
        )
        .bind(&group_name)
        .bind(entity_id)
        .bind(entity_type.as_str())
        .fetch_all(&database.pool)
        .await?;
        let mut values = HashMap::with_capacity(rows.len());
        for row in &rows {
            let stored: String = row.try_get("category")?;
            let value: i32 = row.try_get("value")?;
            values.insert(stored, value);
        }
        for candidate in &candidates {
            if let Some(value) = values.get(candidate) {
                return Ok(*value != 0);
            }
        }
        Ok(false)
    })
}

pub fn check_command_permission(
    group_name: &str,
    entity_id: &str,
    entity_type: EntityType,
    command_name: &str,
) -> Result<bool> {
    let command_name = command_name.trim().trim_start_matches('/');
    if command_name.is_empty() {
        return Ok(false);
    }
    check_permission(group_name, entity_id, entity_type, &format!("command.{command_name}"))
}

fn category_candidates(category: &str) -> Vec<String> {
    let mut candidates = vec![category.to_string()];
    let mut prefix = category;
    while let Some(index) = prefix.rfind('.') {
        prefix = &prefix[..index];
        candidates.push(format!("{prefix}.*"));
    }
    candidates.push("*".to_string());
    candidates
}

fn validate_entity_id(entity_id: &str) -> Result<String> {
    let entity_id = entity_id.trim();
    if entity_id.is_empty() {
        return Err(crate::validation_error!("实体 id 不能为空"));
    }
    if entity_id.chars().count() > 128 {
        return Err(crate::validation_error!("实体 id 不能超过 128 个字符"));
    }
    Ok(entity_id.to_string())
}

fn run_database<T, F>(operation: F) -> Result<T>
where
    T: Send,
    F: AsyncFnOnce(&SystemDatabase) -> Result<T> + Send,
{
    crate::runtime::run_outside_async_runtime(move || with_database(operation))
}

async fn fetch_group(database: &SystemDatabase, name: &str) -> Result<Option<PermissionGroup>> {
    let row = sqlx::query(
        "SELECT g.name, g.created_at, g.updated_at,
            (SELECT COUNT(*) FROM zihuan_settings.permission_entry e WHERE e.group_name = g.name) AS entry_count
         FROM zihuan_settings.permission_group g WHERE g.name = ?",
    )
    .bind(name)
    .fetch_optional(&database.pool)
    .await?;
    row.map(|row| group_from_row(&row)).transpose()
}

fn group_from_row(row: &sqlx::any::AnyRow) -> Result<PermissionGroup> {
    Ok(PermissionGroup {
        name: row.try_get("name")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        entry_count: row.try_get("entry_count")?,
    })
}

async fn fetch_entry(
    database: &SystemDatabase,
    group_name: &str,
    entity_id: &str,
    entity_type: EntityType,
    category: &str,
) -> Result<Option<PermissionEntry>> {
    let row = sqlx::query(
        "SELECT group_name, entity_id, entity_type, category, value, created_at, updated_at
         FROM zihuan_settings.permission_entry
         WHERE group_name = ? AND entity_id = ? AND entity_type = ? AND category = ?",
    )
    .bind(group_name)
    .bind(entity_id)
    .bind(entity_type.as_str())
    .bind(category)
    .fetch_optional(&database.pool)
    .await?;
    row.map(|row| entry_from_row(&row)).transpose()
}

fn entry_from_row(row: &sqlx::any::AnyRow) -> Result<PermissionEntry> {
    let entity_type: String = row.try_get("entity_type")?;
    Ok(PermissionEntry {
        group_name: row.try_get("group_name")?,
        entity_id: row.try_get("entity_id")?,
        entity_type: EntityType::parse(&entity_type)?,
        category: row.try_get("category")?,
        value: row.try_get("value")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
