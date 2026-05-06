use chrono::NaiveDateTime;
use log::{debug, error, info, warn};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::data_value::{MySqlConfig, RedisConfig};

use crate::ConnectionManager;

/// MessageStore provides Redis-backed message storage with MySQL persistence and in-memory fallback.
pub struct MessageStore {
    connection_manager: ConnectionManager,
    memory_store: Arc<Mutex<HashMap<String, String>>>,
    mysql_memory_store: Arc<Mutex<HashMap<String, MessageRecord>>>,
}

impl std::fmt::Debug for MessageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageStore")
            .field("connection_manager", &self.connection_manager)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub content: String,
    pub at_target_list: Option<String>,
    pub media_json: Option<String>,
}

impl MessageStore {
    pub fn new(mysql_ref: Arc<MySqlConfig>, redis_ref: Option<Arc<RedisConfig>>) -> Self {
        let connection_manager = ConnectionManager::new(mysql_ref, redis_ref);

        if connection_manager.mysql_pool().is_none() {
            warn!("[MessageStore] mysql_ref has no pool. Persistent storage will fall back to memory.");
        }
        if connection_manager.redis_ref().is_none() {
            warn!("[MessageStore] No redis_ref provided. Using in-memory message cache.");
        }

        Self {
            connection_manager,
            memory_store: Arc::new(Mutex::new(HashMap::new())),
            mysql_memory_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn mysql_pool(&self) -> Option<&sqlx::mysql::MySqlPool> {
        self.connection_manager.mysql_pool()
    }

    pub async fn load_messages_from_mysql(&self, limit: u32) -> Result<u32> {
        let Some(pool) = self.mysql_pool() else {
            warn!("[MessageStore] No MySQL pool available, skipping message loading");
            return Ok(0);
        };

        let records = sqlx::query(
            r#"
            SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json
            FROM message_record
            ORDER BY send_time DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::StringError(format!("Failed to query messages from MySQL: {}", e)))?;

        info!(
            "[MessageStore] Loaded {} message records from MySQL",
            records.len()
        );

        let mut loaded_count = 0;

        for row in records {
            let message_id: String = row.get("message_id");
            let content: String = row.get("content");

            match self.connection_manager.set_redis_value(&message_id, &content).await {
                Ok(_) => {
                    loaded_count += 1;
                    debug!(
                        "[MessageStore] Loaded message {} into Redis from MySQL",
                        message_id
                    );
                    continue;
                }
                Err(e) if self.connection_manager.redis_ref().is_some() => {
                    error!(
                        "[MessageStore] Failed to load message {} into Redis: {}",
                        message_id, e
                    );
                }
                Err(_) => {}
            }

            let mut mem = self.memory_store.lock().await;
            mem.insert(message_id.clone(), content.clone());
            loaded_count += 1;
            debug!(
                "[MessageStore] Loaded message {} into memory from MySQL",
                message_id
            );
        }

        info!(
            "[MessageStore] Successfully loaded {} messages from MySQL into cache",
            loaded_count
        );
        Ok(loaded_count)
    }

    pub async fn get_messages_by_sender(
        &self,
        sender_id: &str,
        group_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<MessageRecord>> {
        let Some(pool) = self.mysql_pool() else {
            warn!("[MessageStore] No MySQL pool available, checking memory buffer");
            let mem = self.mysql_memory_store.lock().await;
            let mut records: Vec<MessageRecord> = mem
                .values()
                .filter(|r| {
                    r.sender_id == sender_id
                        && (group_id.is_none() || r.group_id.as_deref() == group_id)
                })
                .cloned()
                .collect();

            records.sort_by(|a, b| b.send_time.cmp(&a.send_time));
            records.truncate(limit as usize);

            return Ok(records);
        };

        let records = if let Some(gid) = group_id {
            sqlx::query(
                r#"
                SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json
                FROM message_record
                WHERE sender_id = ? AND group_id = ?
                ORDER BY send_time DESC
                LIMIT ?
                "#,
            )
            .bind(sender_id)
            .bind(gid)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                Error::StringError(format!(
                    "Failed to query messages by sender and group: {}",
                    e
                ))
            })?
        } else {
            sqlx::query(
                r#"
                SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json
                FROM message_record
                WHERE sender_id = ?
                ORDER BY send_time DESC
                LIMIT ?
                "#,
            )
            .bind(sender_id)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::StringError(format!("Failed to query messages by sender: {}", e)))?
        };

        let mut result = Vec::new();
        for row in records {
            result.push(MessageRecord {
                message_id: row.get("message_id"),
                sender_id: row.get("sender_id"),
                sender_name: row.get("sender_name"),
                send_time: row.get("send_time"),
                group_id: row.get("group_id"),
                group_name: row.get("group_name"),
                content: row.get("content"),
                at_target_list: row.get("at_target_list"),
                media_json: row.get("media_json"),
            });
        }

        debug!(
            "[MessageStore] Retrieved {} messages for sender {} (group: {:?})",
            result.len(),
            sender_id,
            group_id
        );
        Ok(result)
    }

    pub async fn store_message(&self, message_id: &str, message: &str) {
        match self
            .connection_manager
            .set_redis_value(message_id, message)
            .await
        {
            Ok(_) => {
                debug!("[MessageStore] Message stored in Redis: {}", message_id);
                return;
            }
            Err(e) if self.connection_manager.redis_ref().is_some() => {
                error!("[MessageStore] Failed to store message in Redis: {}", e);
            }
            Err(_) => {}
        }

        let mut store = self.memory_store.lock().await;
        store.insert(message_id.to_string(), message.to_string());
        debug!("[MessageStore] Message stored in memory: {}", message_id);
    }

    pub async fn store_message_record(&self, record: &MessageRecord) -> Result<()> {
        if let Some(pool) = self.mysql_pool() {
            let result = sqlx::query(
                r#"
                INSERT INTO message_record
                (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&record.message_id)
            .bind(&record.sender_id)
            .bind(&record.sender_name)
            .bind(record.send_time)
            .bind(&record.group_id)
            .bind(&record.group_name)
            .bind(&record.content)
            .bind(&record.at_target_list)
            .bind(&record.media_json)
            .execute(pool)
            .await;

            match result {
                Ok(_) => {
                    debug!(
                        "[MessageStore] Message record persisted to MySQL: {}",
                        record.message_id
                    );
                    return Ok(());
                }
                Err(e) => {
                    error!(
                        "[MessageStore] Failed to store message record in MySQL: {}",
                        e
                    );
                }
            }
        }

        let mut mem = self.mysql_memory_store.lock().await;
        mem.insert(record.message_id.clone(), record.clone());
        debug!(
            "[MessageStore] Message record stored in memory buffer: {}",
            record.message_id
        );
        Ok(())
    }

    pub async fn get_message_record(&self, message_id: &str) -> Result<Option<MessageRecord>> {
        if let Some(pool) = self.mysql_pool() {
            let result = sqlx::query(
                r#"
                SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list
                , media_json
                FROM message_record
                WHERE message_id = ?
                "#,
            )
            .bind(message_id)
            .fetch_optional(pool)
            .await;

            match result {
                Ok(Some(row)) => {
                    let record = MessageRecord {
                        message_id: row.get("message_id"),
                        sender_id: row.get("sender_id"),
                        sender_name: row.get("sender_name"),
                        send_time: row.get("send_time"),
                        group_id: row.get("group_id"),
                        group_name: row.get("group_name"),
                        content: row.get("content"),
                        at_target_list: row.get("at_target_list"),
                        media_json: row.get("media_json"),
                    };
                    debug!(
                        "[MessageStore] Message record retrieved from MySQL: {}",
                        message_id
                    );
                    return Ok(Some(record));
                }
                Ok(None) => {
                    debug!(
                        "[MessageStore] Message record not found in MySQL: {}",
                        message_id
                    );
                }
                Err(e) => {
                    error!(
                        "[MessageStore] Failed to retrieve message record from MySQL: {}",
                        e
                    );
                }
            }
        }

        let mem = self.mysql_memory_store.lock().await;
        Ok(mem.get(message_id).cloned())
    }

    pub async fn get_message(&self, message_id: &str) -> Option<String> {
        match self.connection_manager.get_redis_value(message_id).await {
            Ok(val) => {
                if val.is_some() {
                    return val;
                }
            }
            Err(e) if self.connection_manager.redis_ref().is_some() => {
                error!("[MessageStore] Failed to get message from Redis: {}", e);
            }
            Err(_) => {}
        }

        let store = self.memory_store.lock().await;
        store.get(message_id).cloned()
    }

    pub async fn get_message_with_mysql(&self, message_id: &str) -> Option<String> {
        match self.connection_manager.get_redis_value(message_id).await {
            Ok(val) => {
                if val.is_some() {
                    return val;
                }
            }
            Err(e) if self.connection_manager.redis_ref().is_some() => {
                error!("[MessageStore] Failed to get message from Redis: {}", e);
            }
            Err(_) => {}
        }

        if let Some(pool) = self.mysql_pool() {
            if let Ok(Some(record)) = sqlx::query_as::<_, (String,)>(
                "SELECT content FROM message_record WHERE message_id = ? LIMIT 1",
            )
            .bind(message_id)
            .fetch_optional(pool)
            .await
            {
                debug!(
                    "[MessageStore] Message retrieved from MySQL: {}",
                    message_id
                );
                return Some(record.0);
            }
        }

        let mem = self.mysql_memory_store.lock().await;
        if let Some(rec) = mem.get(message_id) {
            return Some(rec.content.clone());
        }

        let store = self.memory_store.lock().await;
        store.get(message_id).cloned()
    }
}
