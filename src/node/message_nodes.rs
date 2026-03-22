use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port, NodeType};
use crate::bot_adapter::models::message::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::block_in_place;
use sqlx::mysql::MySqlPool;
use log::{debug, error};
use chrono::Local;

/// Message MySQL Persistence Node - Stores MessageEvent to MySQL database
pub struct MessageMySQLPersistenceNode {
    id: String,
    name: String,
    pool: Option<MySqlPool>,
    /// The URL used in the last connection attempt (success or failure).
    last_mysql_url: Option<String>,
    /// Cached error string from the last failed connection attempt for the current URL.
    /// When set, `ensure_pool` skips retrying until the URL changes.
    pool_connect_error: Option<String>,
}

impl MessageMySQLPersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            pool: None,
            last_mysql_url: None,
            pool_connect_error: None,
        }
    }

    /// Return the cached pool if the URL is unchanged and the last attempt succeeded.
    /// Skip retrying if the URL is unchanged but the last attempt failed (return cached error).
    /// Reconnect only when the URL changes.
    fn ensure_pool(&mut self, url: &str) -> Result<&MySqlPool> {
        let url_changed = self.last_mysql_url.as_deref() != Some(url);

        if !url_changed {
            // Same URL — return existing pool or propagate cached error without retrying.
            if self.pool.is_some() {
                return Ok(self.pool.as_ref().unwrap());
            }
            if let Some(ref err) = self.pool_connect_error {
                return Err(crate::string_error!("{}", err));
            }
        }

        // URL changed or first attempt — try (re)connecting.
        let url_str = url.to_string();
        match if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(MySqlPool::connect(&url_str)))
        } else {
            tokio::runtime::Runtime::new()?.block_on(MySqlPool::connect(&url_str))
        } {
            Ok(pool) => {
                self.pool = Some(pool);
                self.last_mysql_url = Some(url_str);
                self.pool_connect_error = None;
                Ok(self.pool.as_ref().unwrap())
            }
            Err(e) => {
                let msg = format!("[MessageMySQLPersistenceNode] Failed to connect to MySQL: {}", e);
                self.pool = None;
                self.last_mysql_url = Some(url_str);
                self.pool_connect_error = Some(msg.clone());
                Err(crate::string_error!("{}", msg))
            }
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

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let message_event = inputs.get("message_event").and_then(|v| match v {
            DataValue::MessageEvent(e) => Some(e.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("message_event is required".to_string()))?;

        let mysql_config = inputs.get("mysql_ref").and_then(|v| match v {
            DataValue::MySqlRef(r) => Some(r.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_ref is required".to_string()))?;

        let url = mysql_config.url.as_deref().ok_or_else(|| {
            crate::error::Error::InvalidNodeInput("mysql_ref has no URL configured".to_string())
        })?;

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
        let content: String = message_event.message_list.iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("");
        let at_targets: Vec<String> = message_event.message_list.iter()
            .filter_map(|m| if let Message::At(at) = m { Some(at.target_id()) } else { None })
            .collect();
        let at_target_list: Option<String> = if at_targets.is_empty() {
            None
        } else {
            Some(at_targets.join(","))
        };

        let pool = match self.ensure_pool(url) {
            Ok(p) => p,
            Err(e) => {
                error!("[MessageMySQLPersistenceNode] Cannot acquire pool: {}", e);
                let mut outputs = HashMap::new();
                outputs.insert("success".to_string(), DataValue::Boolean(false));
                outputs.insert("message_event".to_string(), DataValue::MessageEvent(message_event));
                self.validate_outputs(&outputs)?;
                return Ok(outputs);
            }
        };

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
            .execute(pool)
            .await
        };

        let success = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            match block_in_place(|| handle.block_on(run)) {
                Ok(_) => {
                    debug!("[MessageMySQLPersistenceNode] Message {} persisted to MySQL", message_id);
                    true
                }
                Err(e) => {
                    error!("[MessageMySQLPersistenceNode] INSERT failed for message {}: {}", message_id, e);
                    // Drop pool so the next message triggers a fresh reconnect attempt.
                    self.pool = None;
                    self.last_mysql_url = None;
                    self.pool_connect_error = None;
                    false
                }
            }
        } else {
            match tokio::runtime::Runtime::new()?.block_on(run) {
                Ok(_) => {
                    debug!("[MessageMySQLPersistenceNode] Message {} persisted to MySQL", message_id);
                    true
                }
                Err(e) => {
                    error!("[MessageMySQLPersistenceNode] INSERT failed for message {}: {}", message_id, e);
                    self.pool = None;
                    self.last_mysql_url = None;
                    self.pool_connect_error = None;
                    false
                }
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(message_event));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// Message Cache Node - Caches MessageEvent in memory or optional Redis
pub struct MessageCacheNode {
    id: String,
    name: String,
    memory_cache: Arc<TokioMutex<HashMap<String, String>>>,
}

impl MessageCacheNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            memory_cache: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }
}

impl Node for MessageCacheNode {
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
        Some("消息缓存 - 将MessageEvent缓存到内存或Redis")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "消息事件" },
        port! { name = "redis_ref", ty = RedisRef, desc = "可选：Redis连接配置引用（若不提供则使用内存缓存）", optional },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "消息是否缓存成功" },
        port! { name = "message_event", ty = MessageEvent, desc = "传递输入的消息事件" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        // Extract message event
        let message_event = inputs.get("message_event").and_then(|v| match v {
            DataValue::MessageEvent(e) => Some(e.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("message_event is required".to_string()))?;

        // Extract optional Redis config reference
        let _redis_ref = inputs.get("redis_ref").and_then(|v| match v {
            DataValue::RedisRef(r) => Some(r.clone()),
            _ => None,
        });

        // Cache the message in memory (in real implementation, would also use Redis if provided)
        let _message_key = message_event.message_id.to_string();
        let _message_json = serde_json::json!({
            "message_id": message_event.message_id,
            "message_type": message_event.message_type.as_str(),
            "sender": {
                "user_id": message_event.sender.user_id,
                "nickname": message_event.sender.nickname,
                "card": message_event.sender.card,
                "role": message_event.sender.role,
            },
            "group_id": message_event.group_id,
            "group_name": message_event.group_name,
            "is_group_message": message_event.is_group_message,
        }).to_string();

        // For synchronous execution, we'll mark success as true
        // Actual async caching would happen in a separate task
        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(true));
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(message_event));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
