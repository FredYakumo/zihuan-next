//! Search and deletion of durable QQ conversation turn headers.
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::Row;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;

/// Lightweight turn row for listings: no media, entries or reasoning payload. Also the wire
/// shape of the data explorer's conversation finder listing.
#[derive(Debug, Clone, Serialize)]
pub struct TurnSummary {
    pub turn_id: String,
    pub created_at: String,
    pub user_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_text: Option<String>,
}

/// The `|` character is the LIKE escape, so it must be doubled before the wildcards.
fn like_contains(value: &str) -> String {
    let escaped = value.replace('|', "||").replace('%', "|%").replace('_', "|_");
    format!("%{escaped}%")
}

/// One extra WHERE fragment plus the bind values it needs, shared by the page query and the count.
struct TurnFilter {
    clause: String,
    binds: Vec<String>,
}

impl TurnFilter {
    /// Either a fuzzy text search over the user input and the bot reply, or a time-point match
    /// on the recorded time: a full date (and everything after it) matches as a prefix, while a
    /// bare clock time (`23:49`) matches anywhere inside the stamp.
    fn build(text_query: Option<&str>, time_query: Option<&str>) -> Option<Self> {
        if let Some(time) = time_query.map(str::trim).filter(|value| !value.is_empty()) {
            let date_shaped = time.len() >= 5 && time.as_bytes()[4] == b'-';
            let pattern = if date_shaped {
                format!("{time}%")
            } else {
                format!("%{time}%")
            };
            return Some(Self {
                clause: " AND created_at LIKE ?".to_string(),
                binds: vec![pattern],
            });
        }
        let text = text_query.map(str::trim).filter(|value| !value.is_empty())?;
        let pattern = like_contains(text);
        Some(Self {
            clause: " AND (user_text LIKE ? ESCAPE '|' OR assistant_text LIKE ? ESCAPE '|')"
                .to_string(),
            binds: vec![pattern.clone(), pattern],
        })
    }
}

/// One page of archived turns for one sender, newest first, plus the total match count. Powers
/// the data explorer's conversation finder.
pub fn query_turns(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    text_query: Option<&str>,
    time_query: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<(Vec<TurnSummary>, i64)> {
    let filter = TurnFilter::build(text_query, time_query);
    let clause = filter.as_ref().map(|filter| filter.clause.as_str()).unwrap_or_default();
    let count_sql = format!(
        "SELECT COUNT(*) AS total FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ?{clause}"
    );
    let page_sql = format!(
        "SELECT turn_id, created_at, user_text, assistant_text FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ?{clause} ORDER BY created_at DESC, turn_id DESC LIMIT ? OFFSET ?"
    );

    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive mysql pool is not initialized")
            })?;
            let mut count_query = sqlx::query(&count_sql).bind(agent_id).bind(sender_id);
            let mut page_query = sqlx::query(&page_sql).bind(agent_id).bind(sender_id);
            if let Some(filter) = &filter {
                for value in &filter.binds {
                    count_query = count_query.bind(value);
                    page_query = page_query.bind(value);
                }
            }
            let total =
                zihuan_core::runtime::block_async(async move { count_query.fetch_one(pool).await })
                    .map_err(zihuan_core::error::Error::Database)?
                    .try_get::<i64, _>("total")
                    .map_err(zihuan_core::error::Error::Database)?;
            let rows = zihuan_core::runtime::block_async(async move {
                page_query.bind(limit).bind(offset).fetch_all(pool).await
            })
            .map_err(zihuan_core::error::Error::Database)?;
            let items = rows
                .into_iter()
                .map(|row| TurnSummary {
                    turn_id: row.try_get("turn_id").unwrap_or_default(),
                    created_at: row
                        .try_get::<NaiveDateTime, _>("created_at")
                        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                    user_text: row.try_get("user_text").unwrap_or_default(),
                    assistant_text: row.try_get("assistant_text").unwrap_or(None),
                })
                .collect();
            Ok((items, total))
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive sqlite pool is not initialized")
            })?;
            let mut count_query = sqlx::query(&count_sql).bind(agent_id).bind(sender_id);
            let mut page_query = sqlx::query(&page_sql).bind(agent_id).bind(sender_id);
            if let Some(filter) = &filter {
                for value in &filter.binds {
                    count_query = count_query.bind(value);
                    page_query = page_query.bind(value);
                }
            }
            let total =
                zihuan_core::runtime::block_async(async move { count_query.fetch_one(pool).await })
                    .map_err(zihuan_core::error::Error::Database)?
                    .try_get::<i64, _>("total")
                    .map_err(zihuan_core::error::Error::Database)?;
            let rows = zihuan_core::runtime::block_async(async move {
                page_query.bind(limit).bind(offset).fetch_all(pool).await
            })
            .map_err(zihuan_core::error::Error::Database)?;
            let items = rows
                .into_iter()
                .map(|row| TurnSummary {
                    turn_id: row.try_get("turn_id").unwrap_or_default(),
                    created_at: row.try_get("created_at").unwrap_or_default(),
                    user_text: row.try_get("user_text").unwrap_or_default(),
                    assistant_text: row.try_get("assistant_text").unwrap_or(None),
                })
                .collect();
            Ok((items, total))
        }
    }
}

/// Delete one archived turn; reports whether a row was removed.
pub fn delete_turn(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    turn_id: &str,
) -> Result<bool> {
    let sql =
        "DELETE FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ? AND turn_id = ?";
    let deleted = match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive mysql pool is not initialized")
            })?;
            zihuan_core::runtime::block_async(async move {
                sqlx::query(sql)
                    .bind(agent_id)
                    .bind(sender_id)
                    .bind(turn_id)
                    .execute(pool)
                    .await
            })
            .map_err(zihuan_core::error::Error::Database)?
            .rows_affected()
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive sqlite pool is not initialized")
            })?;
            zihuan_core::runtime::block_async(async move {
                sqlx::query(sql)
                    .bind(agent_id)
                    .bind(sender_id)
                    .bind(turn_id)
                    .execute(pool)
                    .await
            })
            .map_err(zihuan_core::error::Error::Database)?
            .rows_affected()
        }
    };
    Ok(deleted > 0)
}
