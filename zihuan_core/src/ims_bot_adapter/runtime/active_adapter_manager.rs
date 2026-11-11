use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::connection::BotConnection;
use crate::connection::Connection;
use crate::connection_manager::{ConnectionManager as ConnectionManagerTrait, RuntimeInstanceInfo};
use crate::error::{Error, Result};
use crate::storage::S3Ref;
use crate::storage::{find_connection, load_connections, ConnectionConfig, ConnectionKind};
use async_trait::async_trait;
use chrono::Utc;
use log::info;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::ims_bot_adapter::runtime::adapter::SharedBotAdapter;
use crate::ims_bot_adapter::runtime::{build_ims_bot_adapter, parse_ims_bot_adapter_connection};

const BOT_ADAPTER_INSTANCE_IDLE_TIMEOUT_SECS: i64 = 15 * 60;

#[derive(Clone)]
struct ActiveBotAdapterInstance {
    summary: RuntimeInstanceInfo,
    config_updated_at: String,
    adapter: SharedBotAdapter,
    connection: Arc<BotConnection>,
    role_service_ids: HashSet<String>,
    externally_used: bool,
}

pub struct ActiveAdapterManager {
    instances: RwLock<HashMap<String, Vec<ActiveBotAdapterInstance>>>,
    creation: tokio::sync::Mutex<()>,
}

static ACTIVE_ADAPTER_MANAGER: Lazy<ActiveAdapterManager> = Lazy::new(ActiveAdapterManager::new);

impl ActiveAdapterManager {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            creation: tokio::sync::Mutex::new(()),
        }
    }

    pub fn shared() -> &'static Self {
        &ACTIVE_ADAPTER_MANAGER
    }

    async fn create_instance(
        &self,
        config_id: &str,
        object_storage: Option<Arc<S3Ref>>,
    ) -> Result<ActiveBotAdapterInstance> {
        let connections = load_connections()?;
        let connection = find_connection(&connections, config_id)?;
        if !connection.enabled {
            return Err(Error::ValidationError(format!(
                "connection '{}' is disabled",
                connection.name
            )));
        }

        let ConnectionKind::BotAdapter(raw) = &connection.kind else {
            return Err(Error::ValidationError(format!(
                "connection '{}' is not a bot adapter connection",
                connection.name
            )));
        };

        let adapter_connection = parse_ims_bot_adapter_connection(raw)?;
        let adapter = build_ims_bot_adapter(&adapter_connection, object_storage).await;
        let instance_id = Uuid::new_v4().to_string();
        let mut runtime_connection =
            BotConnection::new(connection.id.clone(), connection.name.clone(), adapter.clone())
                .await;
        runtime_connection.connect().await?;

        let now = Utc::now();
        let instance = ActiveBotAdapterInstance {
            summary: RuntimeInstanceInfo {
                instance_id,
                config_id: connection.id.clone(),
                name: connection.name.clone(),
                kind: "bot_adapter".to_string(),
                keep_alive: runtime_connection.keep_alive(),
                heartbeat_interval_secs: runtime_connection
                    .heartbeat_interval()
                    .map(|interval| interval.as_secs()),
                started_at: now,
                last_used_at: now,
                status: runtime_connection.status(),
            },
            config_updated_at: connection.updated_at.clone(),
            adapter,
            connection: Arc::new(runtime_connection),
            role_service_ids: HashSet::new(),
            externally_used: false,
        };
        info!(
            "[active_adapter_manager] instantiated bot adapter instance_id={} config_id={} name='{}'",
            instance.summary.instance_id, instance.summary.config_id, instance.summary.name
        );
        Ok(instance)
    }

    pub async fn find_existing(&self, config_id: &str) -> Option<SharedBotAdapter> {
        let mut instances = self.instances.write().await;
        let instance = instances.get_mut(config_id)?.first_mut()?;
        if instance.connection.is_closed() {
            return None;
        }
        instance.summary.last_used_at = Utc::now();
        Some(instance.adapter.clone())
    }

    pub async fn get_active_bot_adapter_handle(
        &self,
        config_id: &str,
    ) -> Result<crate::ims_bot_adapter::BotAdapterHandle> {
        let adapter = self.get_or_create(config_id).await?;
        let handle: crate::ims_bot_adapter::BotAdapterHandle = adapter;
        Ok(handle)
    }

    pub async fn get_or_create_for_service(
        &self,
        config_id: &str,
        role_service_id: &str,
        object_storage: Option<Arc<S3Ref>>,
    ) -> Result<SharedBotAdapter> {
        self.acquire(config_id, Some(role_service_id), object_storage).await
    }

    async fn acquire(
        &self,
        config_id: &str,
        role_service_id: Option<&str>,
        object_storage: Option<Arc<S3Ref>>,
    ) -> Result<SharedBotAdapter> {
        let _creation = self.creation.lock().await;
        self.cleanup_stale_instances().await?;
        {
            let mut instances = self.instances.write().await;
            if let Some(bucket) = instances.get_mut(config_id) {
                if let Some(instance) = bucket.first_mut() {
                    instance.summary.last_used_at = Utc::now();
                    if let Some(role_service_id) = role_service_id {
                        instance.role_service_ids.insert(role_service_id.to_string());
                    } else {
                        instance.externally_used = true;
                    }
                    if object_storage.is_some() {
                        instance.adapter.lock().await.set_object_storage(object_storage);
                    }
                    return Ok(instance.adapter.clone());
                }
            }
        }

        let mut instance = self.create_instance(config_id, object_storage).await?;
        if let Some(role_service_id) = role_service_id {
            instance.role_service_ids.insert(role_service_id.to_string());
        } else {
            instance.externally_used = true;
        }
        let handle = instance.adapter.clone();
        let mut instances = self.instances.write().await;
        instances.entry(config_id.to_string()).or_default().push(instance);
        Ok(handle)
    }
    /// Release only this role's subscription; shared and explicitly acquired graph connections survive.
    pub async fn release_for_service(&self, role_service_id: &str) -> Result<()> {
        let _creation = self.creation.lock().await;
        let mut instances = self.instances.write().await;
        let mut removed = Vec::new();
        for bucket in instances.values_mut() {
            let mut retained = Vec::new();
            for mut instance in bucket.drain(..) {
                if instance.role_service_ids.remove(role_service_id) {
                    instance
                        .adapter
                        .lock()
                        .await
                        .unregister_event_handler(&format!("qq_chat_service:{role_service_id}"));
                    if instance.role_service_ids.is_empty() && !instance.externally_used {
                        removed.push(instance);
                        continue;
                    }
                }
                retained.push(instance);
            }
            *bucket = retained;
        }
        instances.retain(|_, bucket| !bucket.is_empty());
        drop(instances);
        for instance in removed {
            instance.connection.close().await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ConnectionManagerTrait for ActiveAdapterManager {
    type Handle = SharedBotAdapter;

    async fn get_or_create(&self, config_id: &str) -> Result<Self::Handle> {
        self.acquire(config_id, None, None).await
    }

    async fn list_instances(&self) -> Result<Vec<RuntimeInstanceInfo>> {
        self.cleanup_stale_instances().await?;
        let instances = self.instances.read().await;
        let mut items = instances
            .values()
            .flat_map(|bucket| {
                bucket.iter().map(|item| {
                    let mut summary = item.summary.clone();
                    summary.status = item.connection.status();
                    summary
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(items)
    }

    async fn close_instance(&self, instance_id: &str) -> Result<bool> {
        let _creation = self.creation.lock().await;
        let mut instances = self.instances.write().await;
        for bucket in instances.values_mut() {
            if let Some(index) =
                bucket.iter().position(|item| item.summary.instance_id == instance_id)
            {
                let removed = bucket.remove(index);
                info!(
                    "[active_adapter_manager] force closed bot adapter instance_id={} config_id={} name='{}'",
                    removed.summary.instance_id, removed.summary.config_id, removed.summary.name
                );
                drop(instances);
                removed.connection.close().await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn close_instances_for_config(&self, config_id: &str) -> Result<usize> {
        let _creation = self.creation.lock().await;
        let mut instances = self.instances.write().await;
        let removed = instances.remove(config_id).unwrap_or_default();
        drop(instances);
        for item in &removed {
            item.connection.close().await?;
        }
        Ok(removed.len())
    }

    async fn cleanup_stale_instances(&self) -> Result<usize> {
        let connections = load_connections()?;
        let connection_states = connections
            .iter()
            .map(|item| (item.id.as_str(), (item.enabled, item.updated_at.as_str())))
            .collect::<HashMap<_, _>>();
        let now = Utc::now();
        let mut instances = self.instances.write().await;
        let mut removed = Vec::new();

        for (config_id, bucket) in instances.iter_mut() {
            let (enabled, current_updated_at) =
                connection_states.get(config_id.as_str()).copied().unwrap_or((false, ""));
            let mut retained = Vec::new();
            for item in bucket.drain(..) {
                let config_changed = item.config_updated_at != current_updated_at;
                if !item.connection.is_closed()
                    && item.connection.keep_alive()
                    && enabled
                    && !config_changed
                {
                    retained.push(item);
                    continue;
                }
                let stale = (now - item.summary.last_used_at).num_seconds()
                    >= BOT_ADAPTER_INSTANCE_IDLE_TIMEOUT_SECS;
                if !item.connection.is_closed() && enabled && !stale && !config_changed {
                    retained.push(item);
                } else {
                    info!(
                        "[active_adapter_manager] destroying bot adapter instance_id={} config_id={} name='{}' enabled={} stale={} config_changed={}",
                        item.summary.instance_id,
                        item.summary.config_id,
                        item.summary.name,
                        enabled,
                        stale,
                        config_changed
                    );
                    removed.push(item);
                }
            }
            *bucket = retained;
        }
        instances.retain(|_, bucket| !bucket.is_empty());
        drop(instances);
        for item in &removed {
            item.connection.close().await?;
        }
        Ok(removed.len())
    }
}

pub async fn initialize_enabled_bot_adapters(_connections: &[ConnectionConfig]) {}

pub async fn sync_enabled_bot_adapters(connections: &[ConnectionConfig]) {
    let desired = connections
        .iter()
        .filter(|item| item.enabled && matches!(item.kind, ConnectionKind::BotAdapter(_)))
        .map(|item| (item.id.clone(), item.updated_at.clone()))
        .collect::<HashMap<_, _>>();
    let manager = ActiveAdapterManager::shared();
    let _creation = manager.creation.lock().await;

    let mut removed = Vec::new();
    {
        let mut instances = manager.instances.write().await;
        let config_ids = instances.keys().cloned().collect::<Vec<_>>();
        for config_id in config_ids {
            let Some(bucket) = instances.get_mut(&config_id) else {
                continue;
            };
            let desired_updated_at = desired.get(&config_id).map(String::as_str);
            let mut retained = Vec::new();
            for item in bucket.drain(..) {
                let should_keep = desired_updated_at
                    .map(|updated_at| item.config_updated_at == updated_at)
                    .unwrap_or(false);
                if should_keep {
                    retained.push(item);
                } else {
                    removed.push(item);
                }
            }
            *bucket = retained;
        }
        instances.retain(|_, bucket| !bucket.is_empty());
    }

    for item in removed {
        info!(
            "[active_adapter_manager] closing bot adapter instance_id={} config_id={} name='{}' due to config sync",
            item.summary.instance_id, item.summary.config_id, item.summary.name
        );
        let _ = item.connection.close().await;
    }
}

pub fn register_active_bot_adapter(
    _connection_id: impl Into<String>,
    _adapter: &SharedBotAdapter,
    _task: JoinHandle<()>,
) {
}

pub fn get_active_bot_adapter_handle(
    connection_id: &str,
) -> Option<crate::ims_bot_adapter::BotAdapterHandle> {
    crate::runtime::block_async(
        ActiveAdapterManager::shared().get_active_bot_adapter_handle(connection_id),
    )
    .ok()
}

pub fn has_active_bot_adapter(connection_id: &str) -> bool {
    crate::runtime::block_async(async move {
        ActiveAdapterManager::shared()
            .list_instances()
            .await
            .map(|items| items.iter().any(|item| item.config_id == connection_id))
    })
    .unwrap_or(false)
}

pub fn list_active_bot_adapter_connection_ids() -> Vec<String> {
    crate::runtime::block_async(async move {
        ActiveAdapterManager::shared()
            .list_instances()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.config_id)
            .collect::<Vec<_>>()
    })
}

pub fn stop_active_bot_adapter(connection_id: &str) -> bool {
    crate::runtime::block_async(
        ActiveAdapterManager::shared().close_instances_for_config(connection_id),
    )
    .map(|count| {
        if count > 0 {
            info!(
                "[active_adapter_manager] stopped {} bot adapter instance(s) for config_id={}",
                count, connection_id
            );
            true
        } else {
            false
        }
    })
    .unwrap_or(false)
}

pub async fn ensure_active_bot_adapter(connection: &ConnectionConfig) -> bool {
    ActiveAdapterManager::shared()
        .get_or_create(&connection.id)
        .await
        .map(|_| true)
        .unwrap_or(false)
}

pub async fn list_runtime_bot_adapter_instances() -> Result<Vec<RuntimeInstanceInfo>> {
    ActiveAdapterManager::shared().list_instances().await
}

pub async fn close_runtime_bot_adapter_instance(instance_id: &str) -> Result<bool> {
    ActiveAdapterManager::shared().close_instance(instance_id).await
}
