use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use log::{error, info, warn};
use once_cell::sync::Lazy;
use storage_handler::{find_connection, load_connections, ConnectionConfig, ConnectionKind};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;
use zihuan_core::connection_manager::{
    ConnectionManager as ConnectionManagerTrait, RuntimeConnectionInstanceSummary,
    RuntimeConnectionStatus,
};
use zihuan_core::error::{Error, Result};

use crate::adapter::{BotAdapter, SharedBotAdapter};
use crate::ws_action::ws_send_action_async;
use crate::{build_ims_bot_adapter, parse_ims_bot_adapter_connection};

const BOT_ADAPTER_INSTANCE_IDLE_TIMEOUT_SECS: i64 = 15 * 60;
const BOT_ADAPTER_HEARTBEAT_INTERVAL_SECS: u64 = 30;

#[derive(Clone)]
struct ActiveBotAdapterInstance {
    summary: RuntimeConnectionInstanceSummary,
    adapter: SharedBotAdapter,
    task: Arc<JoinHandle<()>>,
    heartbeat_task: Arc<JoinHandle<()>>,
}

pub struct ActiveAdapterManager {
    instances: RwLock<HashMap<String, Vec<ActiveBotAdapterInstance>>>,
}

static ACTIVE_ADAPTER_MANAGER: Lazy<ActiveAdapterManager> = Lazy::new(ActiveAdapterManager::new);

impl ActiveAdapterManager {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
        }
    }

    pub fn shared() -> &'static Self {
        &ACTIVE_ADAPTER_MANAGER
    }

    fn spawn_keepalive_loop(
        connection_id: String,
        connection_name: String,
        adapter: SharedBotAdapter,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let adapter_for_run = Arc::clone(&adapter);
                match BotAdapter::start(adapter_for_run).await {
                    Ok(()) => {
                        warn!(
                            "[active_adapter_manager] bot adapter '{}' (config_id={}) disconnected, retrying in 2s",
                            connection_name, connection_id
                        );
                    }
                    Err(err) => {
                        error!(
                            "[active_adapter_manager] bot adapter '{}' (config_id={}) exited with error: {}. retrying in 2s",
                            connection_name, connection_id, err
                        );
                    }
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        })
    }

    fn spawn_heartbeat_loop(
        instance_id: String,
        connection_name: String,
        adapter: SharedBotAdapter,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(Duration::from_secs(BOT_ADAPTER_HEARTBEAT_INTERVAL_SECS));
            loop {
                ticker.tick().await;
                match ws_send_action_async(&adapter, "get_login_info", serde_json::json!({})).await {
                    Ok(_) => {
                        log::debug!(
                            "[active_adapter_manager] heartbeat ok for '{}' (instance_id={})",
                            connection_name, instance_id
                        );
                    }
                    Err(err) => {
                        warn!(
                            "[active_adapter_manager] heartbeat failed for '{}' (instance_id={}): {}",
                            connection_name, instance_id, err
                        );
                    }
                }
            }
        })
    }

    async fn create_instance(&self, config_id: &str) -> Result<ActiveBotAdapterInstance> {
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
        let adapter = build_ims_bot_adapter(&adapter_connection, None).await;
        let instance_id = Uuid::new_v4().to_string();
        let task = Arc::new(Self::spawn_keepalive_loop(
            connection.id.clone(),
            connection.name.clone(),
            Arc::clone(&adapter),
        ));
        let heartbeat_task = Arc::new(Self::spawn_heartbeat_loop(
            instance_id.clone(),
            connection.name.clone(),
            Arc::clone(&adapter),
        ));

        let now = Utc::now();
        Ok(ActiveBotAdapterInstance {
            summary: RuntimeConnectionInstanceSummary {
                instance_id,
                config_id: connection.id.clone(),
                name: connection.name.clone(),
                kind: "bot_adapter".to_string(),
                keep_alive: true,
                heartbeat_interval_secs: Some(BOT_ADAPTER_HEARTBEAT_INTERVAL_SECS),
                started_at: now,
                last_used_at: now,
                status: RuntimeConnectionStatus::Running,
            },
            adapter,
            task,
            heartbeat_task,
        })
    }

    pub async fn get_active_bot_adapter_handle(
        &self,
        config_id: &str,
    ) -> Result<zihuan_core::ims_bot_adapter::BotAdapterHandle> {
        let adapter = self.get_or_create(config_id).await?;
        let handle: zihuan_core::ims_bot_adapter::BotAdapterHandle = adapter;
        Ok(handle)
    }
}

#[async_trait]
impl ConnectionManagerTrait for ActiveAdapterManager {
    type Handle = SharedBotAdapter;

    async fn get_or_create(&self, config_id: &str) -> Result<Self::Handle> {
        self.cleanup_stale_instances().await?;
        {
            let mut instances = self.instances.write().await;
            if let Some(bucket) = instances.get_mut(config_id) {
                if let Some(instance) = bucket.first_mut() {
                    instance.summary.last_used_at = Utc::now();
                    return Ok(instance.adapter.clone());
                }
            }
        }

        let instance = self.create_instance(config_id).await?;
        let handle = instance.adapter.clone();
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
                removed.task.abort();
                removed.heartbeat_task.abort();
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn close_instances_for_config(&self, config_id: &str) -> Result<usize> {
        let mut instances = self.instances.write().await;
        let removed = instances.remove(config_id).unwrap_or_default();
        for item in &removed {
            item.task.abort();
            item.heartbeat_task.abort();
        }
        Ok(removed.len())
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
                if item.summary.keep_alive {
                    retained.push(item);
                    continue;
                }
                let stale = (now - item.summary.last_used_at).num_seconds()
                    >= BOT_ADAPTER_INSTANCE_IDLE_TIMEOUT_SECS;
                if enabled && !stale {
                    retained.push(item);
                } else {
                    item.task.abort();
                    item.heartbeat_task.abort();
                    removed += 1;
                }
            }
            *bucket = retained;
        }
        instances.retain(|_, bucket| !bucket.is_empty());
        Ok(removed)
    }
}

pub async fn initialize_enabled_bot_adapters(_connections: &[ConnectionConfig]) {}

pub async fn sync_enabled_bot_adapters(connections: &[ConnectionConfig]) {
    let desired_ids = connections
        .iter()
        .filter(|item| item.enabled && matches!(item.kind, ConnectionKind::BotAdapter(_)))
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let manager = ActiveAdapterManager::shared();
    if let Ok(current) = manager.list_instances().await {
        for item in current {
            if !desired_ids.iter().any(|id| id == &item.config_id) {
                let _ = manager.close_instances_for_config(&item.config_id).await;
            }
        }
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
) -> Option<zihuan_core::ims_bot_adapter::BotAdapterHandle> {
    zihuan_core::runtime::block_async(
        ActiveAdapterManager::shared().get_active_bot_adapter_handle(connection_id),
    )
    .ok()
}

pub fn has_active_bot_adapter(connection_id: &str) -> bool {
    zihuan_core::runtime::block_async(async move {
        ActiveAdapterManager::shared()
            .list_instances()
            .await
            .map(|items| items.iter().any(|item| item.config_id == connection_id))
    })
    .unwrap_or(false)
}

pub fn list_active_bot_adapter_connection_ids() -> Vec<String> {
    zihuan_core::runtime::block_async(async move {
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
    zihuan_core::runtime::block_async(
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

pub async fn list_runtime_bot_adapter_instances() -> Result<Vec<RuntimeConnectionInstanceSummary>> {
    ActiveAdapterManager::shared().list_instances().await
}

pub async fn close_runtime_bot_adapter_instance(instance_id: &str) -> Result<bool> {
    ActiveAdapterManager::shared().close_instance(instance_id).await
}
