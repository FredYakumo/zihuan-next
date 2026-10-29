use chrono::{DateTime, Local, NaiveDateTime, Utc};
use sqlx::Row;

use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};

pub const DEFAULT_ACCESS_LEVEL: u32 = 3;
pub const BOT_ACCESS_LEVEL: u32 = 2;

/// The database is authoritative; caller supplied levels never grant access.
pub async fn access_level(
    connection: &RelationalDbConnection,
    agent_id: &str,
    user_id: &str,
) -> Result<u32> {
    let query = "SELECT access_level, elevated_level, elevated_until FROM user_permission WHERE agent_id = ? AND user_id = ? AND permission_key = 'access_level'";
    let values: Option<(i64, Option<i64>, Option<String>)> = match connection {
        RelationalDbConnection::MySql(config) => sqlx::query(query)
            .bind(agent_id)
            .bind(user_id)
            .fetch_optional(
                config
                    .pool
                    .as_ref()
                    .ok_or_else(|| crate::string_error!("permission database unavailable"))?,
            )
            .await
            .map_err(Error::Database)?
            .map(|row| {
                (
                    i64::from(row.get::<u32, _>("access_level")),
                    row.get::<Option<u32>, _>("elevated_level").map(i64::from),
                    row.get("elevated_until"),
                )
            }),
        RelationalDbConnection::Sqlite(config) => sqlx::query(query)
            .bind(agent_id)
            .bind(user_id)
            .fetch_optional(
                config
                    .pool
                    .as_ref()
                    .ok_or_else(|| crate::string_error!("permission database unavailable"))?,
            )
            .await
            .map_err(Error::Database)?
            .map(|row| {
                (row.get("access_level"), row.get("elevated_level"), row.get("elevated_until"))
            }),
    };
    let Some((base, elevated, until)) = values else {
        return Ok(DEFAULT_ACCESS_LEVEL);
    };
    let active = until.is_some_and(|value| {
        DateTime::parse_from_rfc3339(&value)
            .map(|until| until > Utc::now())
            .unwrap_or_else(|_| {
                NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S")
                    .map(|until| until > Local::now().naive_local())
                    .unwrap_or(false)
            })
    });
    u32::try_from(if active {
        base.min(elevated.unwrap_or(base))
    } else {
        base
    })
    .map_err(|_| Error::ValidationError("invalid user access level".to_string()))
}

pub async fn grant_temporary(
    connection: &RelationalDbConnection,
    agent_id: &str,
    user_id: &str,
    level: u32,
    until: &str,
) -> Result<()> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query("INSERT INTO user_permission (agent_id,user_id,access_level,elevated_level,elevated_until) VALUES (?,?,3,?,?) ON DUPLICATE KEY UPDATE elevated_level=VALUES(elevated_level), elevated_until=VALUES(elevated_until)")
                .bind(agent_id).bind(user_id).bind(level).bind(until).execute(config.pool.as_ref().ok_or_else(|| crate::string_error!("permission database unavailable"))?).await.map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query("INSERT INTO user_permission (agent_id,user_id,access_level,elevated_level,elevated_until) VALUES (?,?,3,?,?) ON CONFLICT(agent_id,user_id,permission_key) DO UPDATE SET elevated_level=excluded.elevated_level,elevated_until=excluded.elevated_until")
                .bind(agent_id).bind(user_id).bind(i64::from(level)).bind(until).execute(config.pool.as_ref().ok_or_else(|| crate::string_error!("permission database unavailable"))?).await.map_err(Error::Database)?;
        }
    }
    Ok(())
}

/// Command policy rows share UserPermission with user levels. Reserved '*' subjects
/// represent channel-wide policies; payload preserves existing QQ/API/custom rule semantics.
pub fn command_records() -> Result<Vec<crate::config::StoredConfigRecord>> {
    let root = crate::config::ConfigCenter::shared().load_root()?;
    for record in &root.configs.command_permissions {
        save_command_record(record)?;
    }
    let legacy_ids: std::collections::HashSet<_> = root
        .configs
        .command_permissions
        .iter()
        .map(|record| record.config_id.clone())
        .collect();
    if !root.configs.command_permissions.is_empty() {
        crate::config::ConfigCenter::shared().update_root(|root| {
            root.configs
                .command_permissions
                .retain(|record| !legacy_ids.contains(&record.config_id));
            Ok(())
        })?;
    }
    crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            let rows = sqlx::query("SELECT payload FROM zihuan_settings.user_permission WHERE agent_id = '*' AND user_id = '*' AND permission_key LIKE 'command:%' ORDER BY permission_key").fetch_all(&database.pool).await?;
            rows.into_iter()
                .map(|row| {
                    serde_json::from_str(row.try_get::<&str, _>("payload")?).map_err(Error::from)
                })
                .collect()
        })
    })
}

pub fn save_command_record(record: &crate::config::StoredConfigRecord) -> Result<()> {
    crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            let statement = if database.mysql {
                "INSERT INTO zihuan_settings.user_permission (agent_id,user_id,permission_key,payload) VALUES ('*','*',?,?) ON DUPLICATE KEY UPDATE payload=VALUES(payload)"
            } else {
                "INSERT INTO zihuan_settings.user_permission (agent_id,user_id,permission_key,payload) VALUES ('*','*',?,?) ON CONFLICT(agent_id,user_id,permission_key) DO UPDATE SET payload=excluded.payload"
            };
            sqlx::query(statement)
                .bind(format!("command:{}", record.config_id))
                .bind(serde_json::to_string(record)?)
                .execute(&database.pool)
                .await?;
            Ok(())
        })
    })
}

pub fn delete_command_record(id: &str) -> Result<bool> {
    crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            Ok(sqlx::query("DELETE FROM zihuan_settings.user_permission WHERE agent_id='*' AND user_id='*' AND permission_key=?").bind(format!("command:{id}")).execute(&database.pool).await?.rows_affected() > 0)
        })
    })
}

pub fn model_api_keys() -> Result<Vec<crate::system_config::ModelHttpApiKey>> {
    crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            let rows = sqlx::query("SELECT payload FROM zihuan_settings.user_permission WHERE agent_id='*' AND permission_key='model_http'").fetch_all(&database.pool).await?;
            rows.into_iter()
                .map(|row| {
                    serde_json::from_str(row.try_get::<&str, _>("payload")?).map_err(Error::from)
                })
                .collect()
        })
    })
}

pub fn replace_model_api_keys(keys: &[crate::system_config::ModelHttpApiKey]) -> Result<()> {
    crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            let mut transaction = database.pool.begin().await?;
            sqlx::query("DELETE FROM zihuan_settings.user_permission WHERE agent_id='*' AND permission_key='model_http'").execute(&mut *transaction).await?;
            for key in keys {
                sqlx::query("INSERT INTO zihuan_settings.user_permission (agent_id,user_id,permission_key,payload) VALUES ('*',?,'model_http',?)").bind(&key.id).bind(serde_json::to_string(key)?).execute(&mut *transaction).await?;
            }
            transaction.commit().await?;
            Ok(())
        })
    })
}

pub async fn set_access_level(
    connection: &RelationalDbConnection,
    agent_id: &str,
    user_id: &str,
    level: u32,
) -> Result<()> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query("INSERT INTO user_permission (agent_id,user_id,access_level) VALUES (?,?,?) ON DUPLICATE KEY UPDATE access_level=VALUES(access_level),elevated_level=NULL,elevated_until=NULL")
                .bind(agent_id).bind(user_id).bind(level).execute(config.pool.as_ref().ok_or_else(|| crate::string_error!("permission database unavailable"))?).await?;
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query("INSERT INTO user_permission (agent_id,user_id,access_level) VALUES (?,?,?) ON CONFLICT(agent_id,user_id,permission_key) DO UPDATE SET access_level=excluded.access_level,elevated_level=NULL,elevated_until=NULL")
                .bind(agent_id).bind(user_id).bind(i64::from(level)).execute(config.pool.as_ref().ok_or_else(|| crate::string_error!("permission database unavailable"))?).await?;
        }
    }
    Ok(())
}

pub fn memory_source_level(access: &super::AgentMemoryAccessContext, sender: &str) -> Result<u32> {
    if let Some(connection) = &access.permission_connection {
        return crate::runtime::block_async(access_level(connection, &access.agent_id, sender));
    }
    crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            let value: Option<i64> = sqlx::query_scalar(if database.mysql { "SELECT CAST(access_level AS SIGNED) FROM zihuan_settings.user_permission WHERE agent_id=? AND user_id=? AND permission_key='access_level'" } else { "SELECT access_level FROM zihuan_settings.user_permission WHERE agent_id=? AND user_id=? AND permission_key='access_level'" }).bind(&access.agent_id).bind(sender).fetch_optional(&database.pool).await?;
            value
                .map(u32::try_from)
                .transpose()
                .map(|level| level.unwrap_or(DEFAULT_ACCESS_LEVEL))
                .map_err(|_| Error::ValidationError("invalid user access level".to_string()))
        })
    })
}
