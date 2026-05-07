use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use log::info;
use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use tokio::sync::RwLock;
use uuid::Uuid;
use zihuan_core::connection_manager::{
    ConnectionManager as ConnectionManagerTrait, RuntimeConnectionInstanceSummary,
    RuntimeConnectionStatus,
};
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::data_value::{MySqlConfig, RedisConfig};
use zihuan_graph_engine::database::weaviate::WeaviateRef;
use zihuan_graph_engine::object_storage::S3Ref;

use crate::resource_resolver::find_connection;
use crate::rustfs::build_s3_ref as build_s3_direct_ref;
use crate::weaviate::build_weaviate_ref as build_weaviate_direct_ref;
use crate::{load_connections, ConnectionKind};

const STORAGE_INSTANCE_IDLE_TIMEOUT_SECS: i64 = 15 * 60;
const DEFAULT_MYSQL_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_WEAVIATE_TIMEOUT_SECS: u64 = 30;

#[derive(Clone)]
pub enum StorageRuntimeHandle {
    MySql(Arc<MySqlConfig>),
    S3(Arc<S3Ref>),
    Weaviate(Arc<WeaviateRef>),
}

#[derive(Clone)]
enum StorageRuntimePayload {
    MySql(Arc<MySqlConfig>),
    S3(Arc<S3Ref>),
    Weaviate(Arc<WeaviateRef>),
}

impl StorageRuntimePayload {
    fn clone_handle(&self) -> StorageRuntimeHandle {
        match self {
            Self::MySql(value) => StorageRuntimeHandle::MySql(value.clone()),
            Self::S3(value) => StorageRuntimeHandle::S3(value.clone()),
            Self::Weaviate(value) => StorageRuntimeHandle::Weaviate(value.clone()),
        }
    }
}

#[derive(Clone)]
struct StorageRuntimeInstance {
    summary: RuntimeConnectionInstanceSummary,
    payload: StorageRuntimePayload,
}

pub struct RuntimeStorageConnectionManager {
    instances: RwLock<HashMap<String, Vec<StorageRuntimeInstance>>>,
}

static RUNTIME_STORAGE_CONNECTION_MANAGER: Lazy<RuntimeStorageConnectionManager> =
    Lazy::new(RuntimeStorageConnectionManager::new);

impl RuntimeStorageConnectionManager {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
        }
    }

    pub fn shared() -> &'static Self {
        &RUNTIME_STORAGE_CONNECTION_MANAGER
    }

    pub async fn get_or_create_mysql_ref(&self, config_id: &str) -> Result<Arc<MySqlConfig>> {
        match self.get_or_create(config_id).await? {
            StorageRuntimeHandle::MySql(value) => Ok(value),
            _ => Err(Error::ValidationError(format!(
                "config '{}' is not a mysql runtime connection",
                config_id
            ))),
        }
    }

    pub async fn get_or_create_s3_ref(&self, config_id: &str) -> Result<Arc<S3Ref>> {
        match self.get_or_create(config_id).await? {
            StorageRuntimeHandle::S3(value) => Ok(value),
            _ => Err(Error::ValidationError(format!(
                "config '{}' is not a rustfs runtime connection",
                config_id
            ))),
        }
    }

    pub async fn get_or_create_weaviate_ref(&self, config_id: &str) -> Result<Arc<WeaviateRef>> {
        match self.get_or_create(config_id).await? {
            StorageRuntimeHandle::Weaviate(value) => Ok(value),
            _ => Err(Error::ValidationError(format!(
                "config '{}' is not a weaviate runtime connection",
                config_id
            ))),
        }
    }

    fn build_runtime_instance(
        &self,
        config_id: &str,
    ) -> Result<(StorageRuntimeInstance, StorageRuntimeHandle)> {
        let connections = load_connections()?;
        let connection = find_connection(&connections, config_id)?;
        if !connection.enabled {
            return Err(Error::ValidationError(format!(
                "connection '{}' is disabled",
                connection.name
            )));
        }

        let started_at = Utc::now();
        let (payload, kind) = match &connection.kind {
            ConnectionKind::Mysql(mysql) => {
                let pool = zihuan_core::runtime::block_async(
                    MySqlPoolOptions::new()
                        .max_connections(DEFAULT_MYSQL_MAX_CONNECTIONS)
                        .min_connections(1)
                        .acquire_timeout(Duration::from_secs(DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS))
                        .connect(&mysql.url),
                )?;
                let config = Arc::new(MySqlConfig {
                    url: Some(mysql.url.clone()),
                    reconnect_max_attempts: None,
                    reconnect_interval_secs: None,
                    pool: Some(pool),
                    runtime_handle: tokio::runtime::Handle::try_current().ok(),
                });
                (StorageRuntimePayload::MySql(config), "mysql".to_string())
            }
            ConnectionKind::Rustfs(rustfs) => {
                let s3_ref = zihuan_core::runtime::block_async(build_s3_direct_ref(
                    &rustfs.endpoint,
                    &rustfs.bucket,
                    &rustfs.access_key,
                    &rustfs.secret_key,
                    &rustfs.region,
                    rustfs.public_base_url.clone(),
                    rustfs.path_style,
                ))?;
                (StorageRuntimePayload::S3(s3_ref), "rustfs".to_string())
            }
            ConnectionKind::Weaviate(weaviate) => {
                let weaviate_ref =
                    build_weaviate_direct_ref(&weaviate.base_url, &weaviate.class_name, false)?;
                (
                    StorageRuntimePayload::Weaviate(weaviate_ref),
                    "weaviate".to_string(),
                )
            }
            other => {
                return Err(Error::ValidationError(format!(
                    "connection '{}' of type '{}' is not managed by storage runtime manager",
                    connection.name,
                    kind_name(other)
                )))
            }
        };

        let summary = RuntimeConnectionInstanceSummary {
            instance_id: Uuid::new_v4().to_string(),
            config_id: connection.id.clone(),
            name: connection.name.clone(),
            kind,
            keep_alive: false,
            heartbeat_interval_secs: None,
            started_at,
            last_used_at: started_at,
            status: RuntimeConnectionStatus::Running,
        };
        info!(
            "[storage_instance_manager] instantiated runtime instance_id={} config_id={} kind={} name='{}'",
            summary.instance_id,
            summary.config_id,
            summary.kind,
            summary.name
        );
        let handle = payload.clone_handle();
        Ok((StorageRuntimeInstance { summary, payload }, handle))
    }

    async fn mark_used_and_clone(
        &self,
        config_id: &str,
        instances: &mut HashMap<String, Vec<StorageRuntimeInstance>>,
    ) -> Option<StorageRuntimeHandle> {
        let bucket = instances.get_mut(config_id)?;
        let first = bucket.first_mut()?;
        first.summary.last_used_at = Utc::now();
        Some(first.payload.clone_handle())
    }
}

#[async_trait]
impl ConnectionManagerTrait for RuntimeStorageConnectionManager {
    type Handle = StorageRuntimeHandle;

    async fn get_or_create(&self, config_id: &str) -> Result<Self::Handle> {
        self.cleanup_stale_instances().await?;
        {
            let mut instances = self.instances.write().await;
            if let Some(handle) = self.mark_used_and_clone(config_id, &mut instances).await {
                return Ok(handle);
            }
        }

        let (instance, handle) = self.build_runtime_instance(config_id)?;
        let mut instances = self.instances.write().await;
        instances
            .entry(config_id.to_string())
            .or_default()
            .push(instance);
        Ok(handle)
    }

    async fn list_instances(&self) -> Result<Vec<RuntimeConnectionInstanceSummary>> {
        self.cleanup_stale_instances().await?;
        let instances = self.instances.read().await;
        let mut items = instances
            .values()
            .flat_map(|bucket| bucket.iter().map(|item| item.summary.clone()))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(items)
    }

    async fn close_instance(&self, instance_id: &str) -> Result<bool> {
        let mut instances = self.instances.write().await;
        for bucket in instances.values_mut() {
            if let Some(index) = bucket
                .iter()
                .position(|item| item.summary.instance_id == instance_id)
            {
                let removed = bucket.remove(index);
                info!(
                    "[storage_instance_manager] force closed runtime instance_id={} config_id={} kind={} name='{}'",
                    removed.summary.instance_id,
                    removed.summary.config_id,
                    removed.summary.kind,
                    removed.summary.name
                );
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn close_instances_for_config(&self, config_id: &str) -> Result<usize> {
        let mut instances = self.instances.write().await;
        Ok(instances
            .remove(config_id)
            .map(|items| items.len())
            .unwrap_or(0))
    }

    async fn cleanup_stale_instances(&self) -> Result<usize> {
        let connections = load_connections()?;
        let now = Utc::now();
        let mut instances = self.instances.write().await;
        let mut removed = 0usize;
        for (config_id, bucket) in instances.iter_mut() {
            let enabled = connections
                .iter()
                .find(|item| item.id == *config_id)
                .map(|item| item.enabled)
                .unwrap_or(false);
            let mut retained = Vec::new();
            for item in bucket.drain(..) {
                let stale = (now - item.summary.last_used_at).num_seconds()
                    >= STORAGE_INSTANCE_IDLE_TIMEOUT_SECS;
                if enabled && !stale {
                    retained.push(item);
                } else {
                    info!(
                        "[storage_instance_manager] destroying idle runtime instance_id={} config_id={} kind={} name='{}' enabled={} stale={}",
                        item.summary.instance_id,
                        item.summary.config_id,
                        item.summary.kind,
                        item.summary.name,
                        enabled,
                        stale
                    );
                    removed += 1;
                }
            }
            *bucket = retained;
        }
        instances.retain(|_, bucket| !bucket.is_empty());
        Ok(removed)
    }
}

pub fn list_runtime_storage_instances() -> Result<Vec<RuntimeConnectionInstanceSummary>> {
    zihuan_core::runtime::block_async(RuntimeStorageConnectionManager::shared().list_instances())
}

pub fn close_runtime_storage_instance(instance_id: &str) -> Result<bool> {
    zihuan_core::runtime::block_async(
        RuntimeStorageConnectionManager::shared().close_instance(instance_id),
    )
}

pub fn cleanup_runtime_storage_instances() -> Result<usize> {
    zihuan_core::runtime::block_async(
        RuntimeStorageConnectionManager::shared().cleanup_stale_instances(),
    )
}

pub struct MessageStoreConnectionAccess {
    mysql_ref: Arc<MySqlConfig>,
    redis_ref: Option<Arc<RedisConfig>>,
}

impl std::fmt::Debug for MessageStoreConnectionAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageStoreConnectionAccess")
            .field("mysql_ref", &self.mysql_ref)
            .field("redis_ref", &self.redis_ref)
            .finish()
    }
}

impl MessageStoreConnectionAccess {
    pub fn new(mysql_ref: Arc<MySqlConfig>, redis_ref: Option<Arc<RedisConfig>>) -> Self {
        Self {
            mysql_ref,
            redis_ref,
        }
    }

    pub fn mysql_ref(&self) -> &Arc<MySqlConfig> {
        &self.mysql_ref
    }

    pub fn redis_ref(&self) -> Option<&Arc<RedisConfig>> {
        self.redis_ref.as_ref()
    }

    pub fn mysql_pool(&self) -> Option<&MySqlPool> {
        get_pool(&self.mysql_ref)
    }

    pub async fn set_redis_value(&self, key: &str, value: &str) -> Result<()> {
        let redis_ref = self
            .redis_ref
            .as_ref()
            .ok_or_else(|| zihuan_core::string_error!("redis_ref not configured"))?;
        crate::redis::set_value(redis_ref, key, value).await
    }

    pub async fn get_redis_value(&self, key: &str) -> Result<Option<String>> {
        let Some(redis_ref) = self.redis_ref.as_ref() else {
            return Ok(None);
        };
        crate::redis::get_value(redis_ref, key).await
    }
}

pub fn get_pool(mysql_ref: &Arc<MySqlConfig>) -> Option<&MySqlPool> {
    mysql_ref.pool.as_ref()
}

fn kind_name(kind: &ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Mysql(_) => "mysql",
        ConnectionKind::Redis(_) => "redis",
        ConnectionKind::Weaviate(_) => "weaviate",
        ConnectionKind::Rustfs(_) => "rustfs",
        ConnectionKind::BotAdapter(_) => "bot_adapter",
        ConnectionKind::Tavily(_) => "tavily",
    }
}

#[allow(dead_code)]
fn _reqwest_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(DEFAULT_WEAVIATE_TIMEOUT_SECS))
        .build()
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_cached_runtime_instance_for_same_config_id() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let manager = RuntimeStorageConnectionManager::new();
            let mysql_ref = Arc::new(MySqlConfig {
                url: Some("mysql://root@localhost/demo".to_string()),
                reconnect_max_attempts: None,
                reconnect_interval_secs: None,
                pool: None,
                runtime_handle: None,
            });
            let started_at = Utc::now();
            {
                let mut guard = manager.instances.write().await;
                guard.insert(
                    "cfg-1".to_string(),
                    vec![StorageRuntimeInstance {
                        summary: RuntimeConnectionInstanceSummary {
                            instance_id: "inst-1".to_string(),
                            config_id: "cfg-1".to_string(),
                            name: "MySQL Default".to_string(),
                            kind: "mysql".to_string(),
                            keep_alive: false,
                            heartbeat_interval_secs: None,
                            started_at,
                            last_used_at: started_at,
                            status: RuntimeConnectionStatus::Running,
                        },
                        payload: StorageRuntimePayload::MySql(Arc::clone(&mysql_ref)),
                    }],
                );
            }

            let mut guard = manager.instances.write().await;
            let handle = manager
                .mark_used_and_clone("cfg-1", &mut guard)
                .await
                .expect("cached handle");
            drop(guard);

            match handle {
                StorageRuntimeHandle::MySql(found) => {
                    assert!(Arc::ptr_eq(&found, &mysql_ref));
                }
                _ => panic!("expected mysql runtime handle"),
            }

            let guard = manager.instances.read().await;
            assert_eq!(guard.get("cfg-1").map(Vec::len), Some(1));
        });
    }
}
