use crate::message_mysql_chunking::{
    split_content_chunks, truncate_field_if_needed, truncate_optional_field_if_needed,
    AT_TARGET_LIST_MAX_CHARS, CONTENT_MAX_CHARS, GROUP_ID_MAX_CHARS, GROUP_NAME_MAX_CHARS,
    MEDIA_JSON_MAX_CHARS, MESSAGE_ID_MAX_CHARS, SENDER_ID_MAX_CHARS, SENDER_NAME_MAX_CHARS,
};
use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use chrono::Local;
use log::{debug, error, info, warn};
use sqlx;
use std::collections::HashMap;
use tokio::task::block_in_place;
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::message::{collect_media_records, Message};

/// Returns true for errors that indicate a dropped/stale connection rather than
/// a SQL-level problem (constraint violation, syntax error, etc.).
fn is_connection_error(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_)
    )
}

/// QQMessage List MySQL Persistence Node — stores a raw Vec<QQMessage> together
/// with caller-supplied metadata into the `message_record` MySQL table.
///
/// Unlike the service-layer persistence, this node does not require a full
/// `MessageEvent`.  The caller must provide `message_id`, `sender_id`, and
/// `sender_name` explicitly.  `group_id` and `group_name` are optional; an
/// absent or empty string value is stored as NULL.
pub struct QQMessageListMySQLPersistenceNode {
    id: String,
    name: String,
}

impl QQMessageListMySQLPersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for QQMessageListMySQLPersistenceNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("QQMessage列表MySQL持久化 - 将Vec<QQMessage>及元数据存储到MySQL数据库")
    }

    node_input![
        port! { name = "qq_message_list", ty = Vec(QQMessage), desc = "要持久化的QQ消息列表" },
        port! { name = "message_id",      ty = String,          desc = "消息ID" },
        port! { name = "sender_id",       ty = String,          desc = "发送者ID" },
        port! { name = "sender_name",     ty = String,          desc = "发送者名称" },
        port! { name = "group_id",        ty = String,          desc = "群ID（可选）", optional },
        port! { name = "group_name",      ty = String,          desc = "群名称（可选）", optional },
        port! { name = "mysql_ref",       ty = MySqlRef,        desc = "MySQL连接配置引用" },
    ];

    node_output![
        port! { name = "success",         ty = Boolean,         desc = "是否存储成功" },
        port! { name = "qq_message_list", ty = Vec(QQMessage),  desc = "透传输入的消息列表" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        // ── Extract qq_message_list ──────────────────────────────────────────
        let (msg_item_type, msg_items) = inputs
            .get("qq_message_list")
            .and_then(|v| match v {
                DataValue::Vec(ty, items) => Some((ty.clone(), items.clone())),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput(
                    "qq_message_list is required".to_string(),
                )
            })?;

        // ── Extract metadata strings ─────────────────────────────────────────
        let raw_message_id = inputs
            .get("message_id")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("message_id is required".to_string())
            })?;
        let message_id = truncate_field_if_needed(
            "message_id",
            raw_message_id.clone(),
            MESSAGE_ID_MAX_CHARS,
            &raw_message_id,
        );

        let sender_id = inputs
            .get("sender_id")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("sender_id is required".to_string())
            })?;
        let sender_id =
            truncate_field_if_needed("sender_id", sender_id, SENDER_ID_MAX_CHARS, &message_id);

        let sender_name = inputs
            .get("sender_name")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("sender_name is required".to_string())
            })?;
        let sender_name = truncate_field_if_needed(
            "sender_name",
            sender_name,
            SENDER_NAME_MAX_CHARS,
            &message_id,
        );

        // Optional: treat absent or empty string as NULL.
        let group_id: Option<String> = inputs.get("group_id").and_then(|v| match v {
            DataValue::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        });
        let group_id = truncate_optional_field_if_needed(
            "group_id",
            group_id,
            GROUP_ID_MAX_CHARS,
            &message_id,
        );

        let group_name: Option<String> = inputs.get("group_name").and_then(|v| match v {
            DataValue::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        });
        let group_name = truncate_optional_field_if_needed(
            "group_name",
            group_name,
            GROUP_NAME_MAX_CHARS,
            &message_id,
        );

        // ── MySQL pool ───────────────────────────────────────────────────────
        let mysql_config = inputs
            .get("mysql_ref")
            .and_then(|v| match v {
                DataValue::MySqlRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("mysql_ref is required".to_string())
            })?;

        let passthrough = DataValue::Vec(msg_item_type, msg_items.clone());

        let pool = match mysql_config.pool.clone() {
            Some(p) => {
                let size = p.size();
                let idle = p.num_idle();
                let in_use = size.saturating_sub(idle as u32);
                debug!(
                    "[QQMessageListMySQLPersistenceNode] pool size={}, idle={}, in-use={}",
                    size, idle, in_use
                );
                if idle == 0 {
                    warn!(
                        "[QQMessageListMySQLPersistenceNode] No idle connections (all {} in-use) — INSERT may stall",
                        in_use
                    );
                }
                p
            }
            None => {
                error!("[QQMessageListMySQLPersistenceNode] mysql_ref has no active pool");
                let mut outputs = HashMap::new();
                outputs.insert("success".to_string(), DataValue::Boolean(false));
                outputs.insert("qq_message_list".to_string(), passthrough);
                self.validate_outputs(&outputs)?;
                return Ok(outputs);
            }
        };

        // ── Build content and at_target_list from messages ───────────────────
        let messages: Vec<Message> = msg_items
            .iter()
            .filter_map(|v| match v {
                DataValue::QQMessage(m) => Some(m.clone()),
                _ => None,
            })
            .collect();

        let content: String = messages
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("");
        let content_chunks = split_content_chunks(&content, CONTENT_MAX_CHARS);

        let at_targets: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                if let Message::At(at) = m {
                    Some(at.target_id())
                } else {
                    None
                }
            })
            .collect();

        let at_target_list: Option<String> = if at_targets.is_empty() {
            None
        } else {
            Some(at_targets.join(","))
        };
        let at_target_list = truncate_optional_field_if_needed(
            "at_target_list",
            at_target_list,
            AT_TARGET_LIST_MAX_CHARS,
            &message_id,
        );
        let media_json = {
            let records = collect_media_records(&messages);
            if records.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&records)?)
            }
        };
        let media_json = truncate_optional_field_if_needed(
            "media_json",
            media_json,
            MEDIA_JSON_MAX_CHARS,
            &message_id,
        );

        let send_time = Local::now().naive_local();
        let message_id_log = message_id.clone();

        info!(
            "[QQMessageListMySQLPersistenceNode] Inserting message {} (sender={}, group={:?}, chunks={})",
            message_id_log, sender_id, group_id, content_chunks.len(),
        );

        // ── Insert with single retry on connection errors ─────────────────────
        let mut success = false;
        for attempt in 1u32..=2 {
            let run = async {
                for (chunk_index, content_chunk) in content_chunks.iter().enumerate() {
                    let chunk_at_target_list = if chunk_index == 0 {
                        at_target_list.as_ref()
                    } else {
                        None
                    };
                    let chunk_media_json = if chunk_index == 0 {
                        media_json.as_ref()
                    } else {
                        None
                    };

                    sqlx::query(
                        r#"
                        INSERT INTO message_record
                        (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&message_id)
                    .bind(&sender_id)
                    .bind(&sender_name)
                    .bind(send_time)
                    .bind(&group_id)
                    .bind(&group_name)
                    .bind(content_chunk)
                    .bind(chunk_at_target_list)
                    .bind(chunk_media_json)
                    .execute(&pool)
                    .await?;
                }

                Ok::<(), sqlx::Error>(())
            };

            let result = if let Some(handle) = mysql_config.runtime_handle.clone() {
                if tokio::runtime::Handle::try_current().is_ok() {
                    block_in_place(|| handle.block_on(run))
                } else {
                    handle.block_on(run)
                }
            } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(run))
            } else {
                tokio::runtime::Runtime::new()?.block_on(run)
            };

            match result {
                Ok(_) => {
                    if attempt > 1 {
                        info!(
                            "[QQMessageListMySQLPersistenceNode] Message {} inserted (attempt {})",
                            message_id_log, attempt
                        );
                    } else {
                        info!(
                            "[QQMessageListMySQLPersistenceNode] Message {} inserted",
                            message_id_log
                        );
                    }
                    success = true;
                    break;
                }
                Err(ref e) if attempt < 2 && is_connection_error(e) => {
                    warn!(
                        "[QQMessageListMySQLPersistenceNode] Message {} attempt {} connection error ({}); retrying",
                        message_id_log, attempt, e
                    );
                }
                Err(e) => {
                    error!(
                        "[QQMessageListMySQLPersistenceNode] INSERT failed for message {} (attempt {}): {}",
                        message_id_log, attempt, e
                    );
                    break;
                }
            }
        }

        if success {
            info!(
                "[QQMessageListMySQLPersistenceNode] success=true for message {}",
                message_id_log
            );
        } else {
            error!(
                "[QQMessageListMySQLPersistenceNode] success=false for message {}",
                message_id_log
            );
        }

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("qq_message_list".to_string(), passthrough);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
