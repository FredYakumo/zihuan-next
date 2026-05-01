use crate::data_value::MySqlConfig;
use chrono::{Duration, NaiveDateTime};
use sqlx::{
    mysql::{MySqlPool, MySqlRow},
    Row,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::task::block_in_place;
use zihuan_core::error::Result;

const HISTORY_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const GAP_THRESHOLD_MINUTES: i64 = 3;

const USER_HISTORY_SQL: &str = r#"
    SELECT sender_id, sender_name, send_time, content
    FROM message_record
    WHERE sender_id = ?
    ORDER BY send_time DESC
    LIMIT ?
    "#;

const USER_HISTORY_WITH_GROUP_SQL: &str = r#"
    SELECT sender_id, sender_name, send_time, content
    FROM message_record
    WHERE sender_id = ? AND group_id = ?
    ORDER BY send_time DESC
    LIMIT ?
    "#;

const GROUP_HISTORY_SQL: &str = r#"
    SELECT sender_id, sender_name, send_time, content
    FROM message_record
    WHERE group_id = ?
    ORDER BY send_time DESC
    LIMIT ?
    "#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageHistoryRecord {
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

pub(crate) fn message_history_record_from_row(row: MySqlRow) -> MessageHistoryRecord {
    MessageHistoryRecord {
        sender_id: row.get("sender_id"),
        sender_name: row.get("sender_name"),
        send_time: row.get("send_time"),
        content: row.get("content"),
    }
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

#[cfg(test)]
mod tests {
    use super::{
        format_history_messages, group_history_query, user_history_query, MessageHistoryRecord,
    };
    use chrono::{NaiveDate, NaiveDateTime};

    fn dt(hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 31)
            .expect("valid date")
            .and_hms_opt(hour, minute, second)
            .expect("valid time")
    }

    fn record(
        sender_name: &str,
        sender_id: &str,
        send_time: NaiveDateTime,
        content: &str,
    ) -> MessageHistoryRecord {
        MessageHistoryRecord {
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            send_time,
            content: content.to_string(),
        }
    }

    #[test]
    fn selects_user_query_without_group_filter() {
        let query = user_history_query(None);
        assert!(query.contains("WHERE sender_id = ?"));
        assert!(!query.contains("group_id = ?"));
    }

    #[test]
    fn selects_user_query_with_group_filter() {
        let query = user_history_query(Some("123456"));
        assert!(query.contains("WHERE sender_id = ? AND group_id = ?"));
    }

    #[test]
    fn selects_group_query() {
        let query = group_history_query();
        assert!(query.contains("WHERE group_id = ?"));
        assert!(!query.contains("sender_id = ? AND group_id = ?"));
    }

    #[test]
    fn formats_empty_history_as_empty_vec() {
        assert!(format_history_messages(Vec::new()).is_empty());
    }

    #[test]
    fn formats_single_message_with_full_timestamp() {
        let messages = format_history_messages(vec![record("小明", "1001", dt(10, 0, 0), "你好")]);
        assert_eq!(
            messages,
            vec![r#"[2026-03-31 10:00:00] 小明(1001)说: "你好""#.to_string()]
        );
    }

    #[test]
    fn keeps_compact_messages_without_gap_prefix() {
        let messages = format_history_messages(vec![
            record("小明", "1001", dt(10, 1, 0), "第二条"),
            record("小明", "1001", dt(10, 0, 0), "第一条"),
        ]);

        assert_eq!(
            messages,
            vec![
                r#"[2026-03-31 10:00:00] 小明(1001)说: "第一条""#.to_string(),
                r#"小明(1001)说: "第二条""#.to_string(),
            ]
        );
    }

    #[test]
    fn inserts_gap_prefix_when_messages_are_far_apart() {
        let messages = format_history_messages(vec![
            record("小明", "1001", dt(12, 10, 0), "第三条"),
            record("小明", "1001", dt(10, 30, 0), "第二条"),
            record("小明", "1001", dt(10, 0, 0), "第一条"),
        ]);

        assert_eq!(
            messages,
            vec![
                r#"[2026-03-31 10:00:00] 小明(1001)说: "第一条""#.to_string(),
                r#"[间隔30分钟后] 小明(1001)说: "第二条""#.to_string(),
                r#"[间隔1小时40分钟后] 小明(1001)说: "第三条""#.to_string(),
            ]
        );
    }

    #[test]
    fn preserves_desc_query_order_by_reversing_to_oldest_first() {
        let messages = format_history_messages(vec![
            record("小明", "1001", dt(10, 2, 0), "第三条"),
            record("小明", "1001", dt(10, 1, 0), "第二条"),
            record("小明", "1001", dt(10, 0, 0), "第一条"),
        ]);

        assert_eq!(
            messages,
            vec![
                r#"[2026-03-31 10:00:00] 小明(1001)说: "第一条""#.to_string(),
                r#"小明(1001)说: "第二条""#.to_string(),
                r#"小明(1001)说: "第三条""#.to_string(),
            ]
        );
    }

    #[test]
    fn supports_multiple_senders_in_group_history() {
        let messages = format_history_messages(vec![
            record("小红", "1002", dt(10, 5, 0), "收到"),
            record("小明", "1001", dt(10, 0, 0), "大家好"),
        ]);

        assert_eq!(
            messages,
            vec![
                r#"[2026-03-31 10:00:00] 小明(1001)说: "大家好""#.to_string(),
                r#"[间隔5分钟后] 小红(1002)说: "收到""#.to_string(),
            ]
        );
    }

    #[test]
    fn preserves_original_content_with_newlines_and_quotes() {
        let messages = format_history_messages(vec![record(
            "小明",
            "1001",
            dt(10, 0, 0),
            "第一行\n他说\"你好\"",
        )]);

        assert_eq!(
            messages,
            vec![format!(
                "[2026-03-31 10:00:00] 小明(1001)说: \"{}\"",
                "第一行\n他说\"你好\""
            )]
        );
    }

    #[test]
    fn formats_day_scale_gaps_with_day_and_hour() {
        let messages = format_history_messages(vec![
            record(
                "小明",
                "1001",
                NaiveDate::from_ymd_opt(2026, 4, 2)
                    .expect("valid date")
                    .and_hms_opt(12, 0, 0)
                    .expect("valid time"),
                "第二条",
            ),
            record(
                "小明",
                "1001",
                NaiveDate::from_ymd_opt(2026, 3, 31)
                    .expect("valid date")
                    .and_hms_opt(10, 0, 0)
                    .expect("valid time"),
                "第一条",
            ),
        ]);

        assert_eq!(messages[1], r#"[间隔2天2小时后] 小明(1001)说: "第二条""#);
    }
}
