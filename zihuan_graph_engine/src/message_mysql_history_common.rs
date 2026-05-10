use crate::data_value::MySqlConfig;
use chrono::{Duration, NaiveDateTime};
use sqlx::{
    mysql::{MySqlPool, MySqlRow},
    Row,
};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::task::block_in_place;
use zihuan_core::error::Result;

const HISTORY_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const GAP_THRESHOLD_MINUTES: i64 = 3;
const HISTORY_CHUNK_FETCH_MULTIPLIER: u32 = 8;

const USER_HISTORY_SQL: &str = r#"
    SELECT id, message_id, sender_id, sender_name, send_time, content
    FROM message_record
    WHERE sender_id = ?
    ORDER BY send_time DESC, id DESC
    LIMIT ?
    "#;

const USER_HISTORY_WITH_GROUP_SQL: &str = r#"
    SELECT id, message_id, sender_id, sender_name, send_time, content
    FROM message_record
    WHERE sender_id = ? AND group_id = ?
    ORDER BY send_time DESC, id DESC
    LIMIT ?
    "#;

const GROUP_HISTORY_SQL: &str = r#"
    SELECT id, message_id, sender_id, sender_name, send_time, content
    FROM message_record
    WHERE group_id = ?
    ORDER BY send_time DESC, id DESC
    LIMIT ?
    "#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageHistoryChunkRow {
    pub id: i64,
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageHistoryRecord {
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub content: String,
}

pub(crate) fn user_history_query(group_id: Option<&str>) -> &'static str {
    if group_id.is_some() {
        USER_HISTORY_WITH_GROUP_SQL
    } else {
        USER_HISTORY_SQL
    }
}

pub(crate) fn group_history_query() -> &'static str {
    GROUP_HISTORY_SQL
}

pub(crate) fn history_query_row_limit(message_limit: u32) -> i64 {
    i64::from(message_limit.saturating_mul(HISTORY_CHUNK_FETCH_MULTIPLIER))
}

pub(crate) fn message_history_chunk_row_from_row(row: MySqlRow) -> MessageHistoryChunkRow {
    MessageHistoryChunkRow {
        id: row.get("id"),
        message_id: row.get("message_id"),
        sender_id: row.get("sender_id"),
        sender_name: row.get("sender_name"),
        send_time: row.get("send_time"),
        content: row.get("content"),
    }
}

pub(crate) fn aggregate_history_rows(
    rows: Vec<MessageHistoryChunkRow>,
    message_limit: usize,
) -> Vec<MessageHistoryRecord> {
    if rows.is_empty() || message_limit == 0 {
        return Vec::new();
    }

    let mut aggregated = Vec::new();
    let mut current: Option<MessageHistoryRecord> = None;
    let mut chunk_buffer = VecDeque::new();

    for row in rows {
        match current.as_mut() {
            Some(current_record) if current_record.message_id == row.message_id => {
                chunk_buffer.push_front(row.content);
            }
            Some(_) => {
                if let Some(mut finished) = current.take() {
                    finished.content = chunk_buffer.into_iter().collect::<String>();
                    aggregated.push(finished);
                    if aggregated.len() == message_limit {
                        return aggregated;
                    }
                }

                chunk_buffer = VecDeque::from([row.content]);
                current = Some(MessageHistoryRecord {
                    message_id: row.message_id,
                    sender_id: row.sender_id,
                    sender_name: row.sender_name,
                    send_time: row.send_time,
                    content: String::new(),
                });
            }
            None => {
                chunk_buffer.push_front(row.content);
                current = Some(MessageHistoryRecord {
                    message_id: row.message_id,
                    sender_id: row.sender_id,
                    sender_name: row.sender_name,
                    send_time: row.send_time,
                    content: String::new(),
                });
            }
        }
    }

    if let Some(mut finished) = current {
        finished.content = chunk_buffer.into_iter().collect::<String>();
        aggregated.push(finished);
    }

    aggregated.truncate(message_limit);
    aggregated
}

pub(crate) fn run_mysql_query<T, F>(mysql_config: &Arc<MySqlConfig>, query_fn: F) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a MySqlPool,
    ) -> Pin<
        Box<dyn Future<Output = std::result::Result<T, sqlx::Error>> + Send + 'a>,
    >,
{
    let pool = mysql_config.pool.clone().ok_or_else(|| {
        zihuan_core::error::Error::ValidationError(
            "mysql_ref has no active pool — ensure the MySqlNode is connected".to_string(),
        )
    })?;

    let query_future = query_fn(&pool);
    let result = if let Some(handle) = mysql_config.runtime_handle.clone() {
        if tokio::runtime::Handle::try_current().is_ok() {
            block_in_place(|| handle.block_on(query_future))
        } else {
            handle.block_on(query_future)
        }
    } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(query_future))
    } else {
        tokio::runtime::Runtime::new()?.block_on(query_future)
    };

    Ok(result?)
}

pub(crate) fn format_history_messages(mut records: Vec<MessageHistoryRecord>) -> Vec<String> {
    if records.is_empty() {
        return Vec::new();
    }

    records.reverse();

    let mut messages = Vec::with_capacity(records.len());
    let mut previous_send_time = None;

    for record in records {
        let body = format!(
            "{}({})说: \"{}\"",
            record.sender_name, record.sender_id, record.content
        );

        let rendered = match previous_send_time {
            None => format!(
                "[{}] {}",
                record.send_time.format(HISTORY_TIME_FORMAT),
                body
            ),
            Some(previous_send_time) => {
                let gap = record.send_time - previous_send_time;
                if gap >= Duration::minutes(GAP_THRESHOLD_MINUTES) {
                    format!("[间隔{}后] {}", format_gap(gap), body)
                } else {
                    body
                }
            }
        };

        previous_send_time = Some(record.send_time);
        messages.push(rendered);
    }

    messages
}

pub(crate) struct SearchMessagesQueryBuilder {
    pub sender_id: Option<String>,
    pub group_id: Option<String>,
    pub contain: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub sort_by_time_desc: bool,
    pub limit: u32,
}

impl SearchMessagesQueryBuilder {
    pub fn build(&self) -> (String, Vec<String>) {
        let mut where_clauses = Vec::new();
        let mut params = Vec::new();

        if let Some(ref sender_id) = self.sender_id {
            where_clauses.push("sender_id = ?".to_string());
            params.push(sender_id.clone());
        }
        if let Some(ref group_id) = self.group_id {
            where_clauses.push("group_id = ?".to_string());
            params.push(group_id.clone());
        }
        if let Some(ref contain) = self.contain {
            where_clauses.push("content LIKE ?".to_string());
            params.push(format!("%{contain}%"));
        }
        if let Some(ref start_time) = self.start_time {
            where_clauses.push("send_time >= ?".to_string());
            params.push(start_time.clone());
        }
        if let Some(ref end_time) = self.end_time {
            where_clauses.push("send_time <= ?".to_string());
            params.push(end_time.clone());
        }

        let order = if self.sort_by_time_desc {
            "ORDER BY send_time DESC, id DESC"
        } else {
            "ORDER BY send_time ASC, id ASC"
        };

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let sql = format!(
            "SELECT id, message_id, sender_id, sender_name, send_time, content FROM message_record {where_sql} {order} LIMIT ?"
        );
        params.push(history_query_row_limit(self.limit).to_string());

        (sql, params)
    }
}

fn format_gap(duration: Duration) -> String {
    let total_minutes = duration.num_minutes().max(0);
    if total_minutes < 60 {
        return format!("{total_minutes}分钟");
    }

    let total_hours = duration.num_hours().max(0);
    if total_hours < 24 {
        let minutes = total_minutes - total_hours * 60;
        return format!("{total_hours}小时{minutes}分钟");
    }

    let days = duration.num_days().max(0);
    let hours = total_hours - days * 24;
    format!("{days}天{hours}小时")
}
