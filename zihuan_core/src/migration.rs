use std::io::{BufRead, BufReader};

use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::error::Result;
use crate::model_inference::model_usage::ModelUsageRecord;
use crate::system_database::SystemDatabase;

struct Migration {
    name: &'static str,
    mysql: &'static str,
    sqlite: &'static str,
    data: DataMigration,
}

enum DataMigration {
    ModelUsages,
}

const MIGRATIONS: &[Migration] = &[Migration {
    name: "001_model_usages",
    mysql: "CREATE TABLE IF NOT EXISTS zihuan_settings.model_usages (
        id VARCHAR(80) PRIMARY KEY, config_id VARCHAR(255) NOT NULL,
        used_at VARCHAR(40) NOT NULL, record_json LONGBLOB NOT NULL,
        input_tokens BIGINT NOT NULL, output_tokens BIGINT NOT NULL,
        cached_input_tokens BIGINT NOT NULL, total_tokens BIGINT NOT NULL,
        cache_hit_rate DOUBLE NULL, tokens_per_second DOUBLE NULL, first_token_ms DOUBLE NULL,
        INDEX model_usages_config_time (config_id, used_at))",
    sqlite: "CREATE TABLE IF NOT EXISTS zihuan_settings.model_usages (
        id TEXT PRIMARY KEY, config_id TEXT NOT NULL, used_at TEXT NOT NULL,
        record_json BLOB NOT NULL, input_tokens INTEGER NOT NULL, output_tokens INTEGER NOT NULL,
        cached_input_tokens INTEGER NOT NULL, total_tokens INTEGER NOT NULL,
        cache_hit_rate REAL, tokens_per_second REAL, first_token_ms REAL)",
    data: DataMigration::ModelUsages,
}];

pub async fn prepare(database: &SystemDatabase) -> Result<()> {
    if database.mysql {
        sqlx::query("CREATE DATABASE IF NOT EXISTS zihuan_settings")
            .execute(&database.pool)
            .await?;
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS zihuan_settings.migrations (name VARCHAR(100) PRIMARY KEY)",
    )
    .execute(&database.pool)
    .await?;
    for migration in MIGRATIONS {
        if !applied(database, migration.name).await? {
            sqlx::query(if database.mysql {
                migration.mysql
            } else {
                migration.sqlite
            })
            .execute(&database.pool)
            .await?;
            if !database.mysql {
                sqlx::query("CREATE INDEX IF NOT EXISTS zihuan_settings.model_usages_config_time ON model_usages (config_id, used_at)")
                    .execute(&database.pool).await?;
            }
            mark_applied(database, migration.name).await?;
        }
    }
    Ok(())
}

async fn applied(database: &SystemDatabase, name: &str) -> Result<bool> {
    Ok(sqlx::query("SELECT name FROM zihuan_settings.migrations WHERE name = ?")
        .bind(name)
        .fetch_optional(&database.pool)
        .await?
        .is_some())
}

async fn mark_applied(database: &SystemDatabase, name: &str) -> Result<()> {
    sqlx::query("INSERT INTO zihuan_settings.migrations (name) VALUES (?)")
        .bind(name)
        .execute(&database.pool)
        .await?;
    Ok(())
}

pub async fn insert_usage(
    database: &SystemDatabase,
    executor: &mut sqlx::AnyConnection,
    id: &str,
    record: &ModelUsageRecord,
) -> Result<()> {
    let usage = record.usage.as_ref();
    let input = usage.and_then(|usage| usage.prompt_tokens).unwrap_or(0) as i64;
    let output = usage.and_then(|usage| usage.completion_tokens).unwrap_or(0) as i64;
    let cached = usage.and_then(|usage| usage.cached_prompt_tokens).unwrap_or(0) as i64;
    let total = usage
        .and_then(|usage| usage.total_tokens)
        .map(|value| value as i64)
        .unwrap_or(input + output);
    let cache_hit_rate = usage.and_then(|usage| {
        let input = usage.prompt_tokens?;
        let cached = usage.cached_prompt_tokens?;
        (input > 0).then(|| cached.min(input) as f64 / input as f64)
    });
    let sql = if database.mysql {
        "INSERT INTO zihuan_settings.model_usages (id, config_id, used_at, record_json,
         input_tokens, output_tokens, cached_input_tokens, total_tokens, cache_hit_rate, tokens_per_second, first_token_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE id = VALUES(id)"
    } else {
        "INSERT INTO zihuan_settings.model_usages (id, config_id, used_at, record_json,
         input_tokens, output_tokens, cached_input_tokens, total_tokens, cache_hit_rate, tokens_per_second, first_token_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING"
    };
    sqlx::query(sql)
        .bind(id)
        .bind(&record.config_id)
        .bind(record.used_at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true))
        .bind(serde_json::to_vec(record)?)
        .bind(input)
        .bind(output)
        .bind(cached)
        .bind(total)
        .bind(cache_hit_rate)
        .bind(record.output_tokens_per_second)
        .bind(record.time_to_first_token_ms)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn import_legacy_usage(database: &SystemDatabase) -> Result<()> {
    const NAME: &str = "002_import_usage_jsonl";
    if applied(database, NAME).await? {
        return Ok(());
    }
    let path = crate::system_config::application_data_dir().join("model_usage/usage.jsonl");
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return mark_applied(database, NAME).await;
        }
        Err(error) => return Err(error.into()),
    };
    let mut transaction = database.pool.begin().await?;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        let record: ModelUsageRecord = serde_json::from_str(&line)?;
        let id = format!("legacy-{:x}", Sha256::digest(format!("{index}:{line}").as_bytes()));
        insert_usage(database, &mut transaction, &id, &record).await?;
    }
    sqlx::query("INSERT INTO zihuan_settings.migrations (name) VALUES (?)")
        .bind(NAME)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn skip_legacy_usage(database: &SystemDatabase) -> Result<()> {
    if !applied(database, "002_import_usage_jsonl").await? {
        mark_applied(database, "002_import_usage_jsonl").await?;
    }
    Ok(())
}

async fn migrate_model_usages(
    source: &SystemDatabase,
    target: &SystemDatabase,
    transaction: &mut sqlx::AnyConnection,
) -> Result<()> {
    let mut last_id = String::new();
    loop {
        let rows = sqlx::query("SELECT id, record_json FROM zihuan_settings.model_usages WHERE id > ? ORDER BY id LIMIT 500")
            .bind(&last_id).fetch_all(&source.pool).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            last_id = row.try_get("id")?;
            let record: ModelUsageRecord =
                serde_json::from_slice(row.try_get::<&[u8], _>("record_json")?)?;
            insert_usage(target, transaction, &last_id, &record).await?;
        }
    }
    Ok(())
}

pub async fn migrate(source: &SystemDatabase, target: &SystemDatabase) -> Result<()> {
    let mut transaction = target.pool.begin().await?;
    for migration in MIGRATIONS {
        match migration.data {
            DataMigration::ModelUsages => {
                migrate_model_usages(source, target, &mut transaction).await?
            }
        }
    }
    for row in sqlx::query("SELECT name FROM zihuan_settings.migrations")
        .fetch_all(&source.pool)
        .await?
    {
        let name: String = row.try_get("name")?;
        let sql = if target.mysql {
            "INSERT INTO zihuan_settings.migrations (name) VALUES (?) ON DUPLICATE KEY UPDATE name = VALUES(name)"
        } else {
            "INSERT INTO zihuan_settings.migrations (name) VALUES (?) ON CONFLICT(name) DO NOTHING"
        };
        sqlx::query(sql).bind(name).execute(&mut *transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}
