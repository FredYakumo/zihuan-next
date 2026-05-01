use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use chrono::Local;
use log::{debug, error, info, warn};
use sqlx;
use std::collections::HashMap;
use tokio::task::block_in_place;
use zihuan_bot_types::message::Message;
use zihuan_core::error::Result;

/// Returns true for errors that indicate a dropped/stale connection rather than
/// a SQL-level problem (constraint violation, syntax error, etc.).
/// On such errors the persistence node retries once; sqlx will have already
/// evicted the bad connection from the pool so the retry gets a fresh one.
fn is_connection_error(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_)
    )
}

/// Message MySQL Persistence Node - Stores MessageEvent to MySQL database.
/// This node is stateless with respect to the pool: it relies on the MySqlNode
/// (upstream) to own and maintain the connection pool, which arrives via the
/// `mysql_ref` input port.
pub struct MessageMySQLPersistenceNode {
    id: String,
    name: String,
}

impl MessageMySQLPersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageMySQLPersistenceNode {
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
        Some("消息MySQL持久化 - 将MessageEvent存储到MySQL数据库")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "消息事件" },
        port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL连接配置引用" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "消息是否存储成功" },
        port! { name = "message_event", ty = MessageEvent, desc = "传递输入的消息事件" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let message_event = inputs
            .get("message_event")
            .and_then(|v| match v {
                DataValue::MessageEvent(e) => Some(e.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("message_event is required".to_string())
            })?;

        let mysql_config = inputs
            .get("mysql_ref")
            .and_then(|v| match v {
                DataValue::MySqlRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("mysql_ref is required".to_string())
            })?;

        // Obtain the live pool from MySqlRef (maintained by the upstream MySqlNode).
        let pool = match mysql_config.pool.clone() {
            Some(p) => {
                let size = p.size();
                let idle = p.num_idle();
                let in_use = size.saturating_sub(idle as u32);
                debug!(
                    "[MessageMySQLPersistenceNode] Received pool from MySqlRef \
                     (size={}, idle={}, in-use={})",
                    size, idle, in_use
                );
                if idle == 0 {
                    warn!(
                        "[MessageMySQLPersistenceNode] No idle connections in pool \
                         (all {} in-use) — INSERT may stall waiting for one to free up",
                        in_use
                    );
                }
                p
            }
            None => {
                error!("[MessageMySQLPersistenceNode] mysql_ref has no active pool — ensure the MySqlNode is connected");
                let mut outputs = HashMap::new();
                outputs.insert("success".to_string(), DataValue::Boolean(false));
                outputs.insert(
                    "message_event".to_string(),
                    DataValue::MessageEvent(message_event),
                );
                self.validate_outputs(&outputs)?;
                return Ok(outputs);
            }
        };

        // Build record fields from MessageEvent
        let message_id = message_event.message_id.to_string();
        let sender_id = message_event.sender.user_id.to_string();
        let sender_name = if message_event.sender.card.is_empty() {
            message_event.sender.nickname.clone()
        } else {
            message_event.sender.card.clone()
        };
        let send_time = Local::now().naive_local();
        let group_id = message_event.group_id.map(|id| id.to_string());
        let group_name = message_event.group_name.clone();
        let content: String = message_event
            .message_list
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("");
        let at_targets: Vec<String> = message_event
            .message_list
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

        // Keep a copy for use in log calls after the async block consumes the originals.
        let message_id_log = message_id.clone();

        info!(
            "[MessageMySQLPersistenceNode] Inserting message {} (sender={}, group={:?}) into MySQL",
            message_id_log, sender_id, group_id,
        );

        // Retry once on connection-level errors (PoolTimedOut, Io, etc.).
        // sqlx automatically evicts the bad connection after a failed query,
        // so the retry will acquire a fresh connection.
        let mut success = false;
        for attempt in 1u32..=2 {
            // Borrow (not move) all locals so the loop body can run twice.
            let run = async {
                sqlx::query(
                    r#"
                    INSERT INTO message_record
                    (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&message_id)
                .bind(&sender_id)
                .bind(&sender_name)
                .bind(send_time)
                .bind(&group_id)
                .bind(&group_name)
                .bind(&content)
                .bind(&at_target_list)
                .execute(&pool)
                .await
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
                            "[MessageMySQLPersistenceNode] Message {} inserted successfully (attempt {})",
                            message_id_log, attempt
                        );
                    } else {
                        info!(
                            "[MessageMySQLPersistenceNode] Message {} inserted successfully",
                            message_id_log
                        );
                    }
                    success = true;
                    break;
                }
                Err(ref e) if attempt < 2 && is_connection_error(e) => {
                    warn!(
                        "[MessageMySQLPersistenceNode] Message {} attempt {} failed with connection error \
                         ({}); sqlx will evict the bad connection — retrying immediately",
                        message_id_log, attempt, e
                    );
                    // Continue to attempt 2.
                }
                Err(e) => {
                    error!(
                        "[MessageMySQLPersistenceNode] INSERT failed for message {} (attempt {}): {}",
                        message_id_log, attempt, e
                    );
                    break;
                }
            }
        }

        if success {
            info!(
                "[MessageMySQLPersistenceNode] Returning success=true for message {}",
                message_id_log
            );
        } else {
            error!(
                "[MessageMySQLPersistenceNode] Returning success=false for message {}",
                message_id_log
            );
        }
        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert(
            "message_event".to_string(),
            DataValue::MessageEvent(message_event),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
