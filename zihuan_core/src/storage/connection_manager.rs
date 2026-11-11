use crate::connection_manager::{ConnectionManager as ConnectionManagerTrait, RuntimeInstanceInfo};
use crate::error::{Error, Result};
use crate::storage::{find_connection, load_connections, ConnectionHandle};
use async_trait::async_trait;
use chrono::Utc;
use log::info;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

const CONNECTION_IDLE_TIMEOUT_SECS: i64 = 15 * 60;

#[derive(Clone)]
struct ManagedConnectionInstance {
    summary: RuntimeInstanceInfo,
    payload: ConnectionHandle,
    /// Distinguishes multiple runtime instances built from one config that differ by
    /// the schema they serve; empty for configs that resolve to a single instance.
    variant: String,
    config_updated_at: String,
}

pub struct RuntimeConnectionManager {
    instances: RwLock<HashMap<String, Vec<ManagedConnectionInstance>>>,
    creation: tokio::sync::Mutex<()>,
}

static RUNTIME_CONNECTION_MANAGER: Lazy<RuntimeConnectionManager> =
    Lazy::new(RuntimeConnectionManager::new);

impl RuntimeConnectionManager {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            creation: tokio::sync::Mutex::new(()),
        }
    }

    pub fn shared() -> &'static Self {
        &RUNTIME_CONNECTION_MANAGER
    }

    async fn build_runtime_instance(
        &self,
        config_id: &str,
        variant: &str,
    ) -> Result<(ManagedConnectionInstance, ConnectionHandle)> {
        let connections = load_connections()?;
        let connection = find_connection(&connections, config_id)?;
        if !connection.enabled {
            return Err(Error::ValidationError(format!(
                "connection '{}' is disabled",
                connection.name
            )));
        }

        let started_at = Utc::now();
        let payload = ConnectionHandle::from_config(connection, variant).await?;

        let summary = RuntimeInstanceInfo {
            instance_id: Uuid::new_v4().to_string(),
            config_id: connection.id.clone(),
            name: connection.name.clone(),
            kind: connection.kind.type_name().to_string(),
            keep_alive: payload.connection().keep_alive(),
            heartbeat_interval_secs: payload
                .connection()
                .heartbeat_interval()
                .map(|interval| interval.as_secs()),
            started_at,
            last_used_at: started_at,
            status: payload.connection().status(),
        };
        info!(
            "[connection_manager] instantiated runtime instance_id={} config_id={} kind={} name='{}' variant='{}'",
            summary.instance_id, summary.config_id, summary.kind, summary.name, variant
        );
        let handle = payload.clone();
        Ok((
            ManagedConnectionInstance {
                summary,
                payload,
                variant: variant.to_string(),
                config_updated_at: connection.updated_at.clone(),
            },
            handle,
        ))
    }

    async fn mark_used_and_clone(
        &self,
        config_id: &str,
        variant: &str,
        instances: &mut HashMap<String, Vec<ManagedConnectionInstance>>,
    ) -> Option<ConnectionHandle> {
        let bucket = instances.get_mut(config_id)?;
        let instance = bucket.iter_mut().find(|instance| instance.variant == variant)?;
        if instance.payload.connection().is_closed() {
            return None;
        }
        instance.summary.last_used_at = Utc::now();
        Some(instance.payload.clone())
    }

    pub async fn get_or_create_variant(
        &self,
        config_id: &str,
        variant: &str,
    ) -> Result<ConnectionHandle> {
        let _creation = self.creation.lock().await;
        self.cleanup_stale_instances().await?;
        {
            let mut instances = self.instances.write().await;
            if let Some(handle) = self.mark_used_and_clone(config_id, variant, &mut instances).await
            {
                return Ok(handle);
            }
        }

        let (instance, handle) = self.build_runtime_instance(config_id, variant).await?;
        let mut instances = self.instances.write().await;
        instances.entry(config_id.to_string()).or_default().push(instance);
        Ok(handle)
    }
}

#[async_trait]
impl ConnectionManagerTrait for RuntimeConnectionManager {
    type Handle = ConnectionHandle;

    async fn get_or_create(&self, config_id: &str) -> Result<Self::Handle> {
        self.get_or_create_variant(config_id, "").await
    }

    async fn list_instances(&self) -> Result<Vec<RuntimeInstanceInfo>> {
        self.cleanup_stale_instances().await?;
        let instances = self.instances.read().await;
        let mut items = instances
            .values()
            .flat_map(|bucket| {
                bucket.iter().map(|item| {
                    let mut summary = item.summary.clone();
                    summary.status = item.payload.connection().status();
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
                    "[connection_manager] force closed runtime instance_id={} config_id={} kind={} name='{}'",
                    removed.summary.instance_id, removed.summary.config_id, removed.summary.kind, removed.summary.name
                );
                drop(instances);
                removed.payload.connection().close().await?;
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
            item.payload.connection().close().await?;
        }
        Ok(removed.len())
    }

    async fn cleanup_stale_instances(&self) -> Result<usize> {
        let connections = load_connections()?;
        let now = Utc::now();
        let mut instances = self.instances.write().await;
        let mut removed = Vec::new();
        for (config_id, bucket) in instances.iter_mut() {
            let current = connections.iter().find(|item| item.id == *config_id);
            let enabled = current.map(|item| item.enabled).unwrap_or(false);
            let mut retained = Vec::new();
            for item in bucket.drain(..) {
                let stale =
                    (now - item.summary.last_used_at).num_seconds() >= CONNECTION_IDLE_TIMEOUT_SECS;
                let externally_held = item.payload.has_external_refs();
                let config_changed = current
                    .map(|config| config.updated_at != item.config_updated_at)
                    .unwrap_or(true);
                if enabled
                    && !config_changed
                    && !item.payload.connection().is_closed()
                    && (!stale || externally_held)
                {
                    retained.push(item);
                } else {
                    info!(
                        "[connection_manager] destroying idle runtime instance_id={} config_id={} kind={} name='{}' enabled={} stale={} externally_held={}",
                        item.summary.instance_id,
                        item.summary.config_id,
                        item.summary.kind,
                        item.summary.name,
                        enabled,
                        stale,
                        externally_held
                    );
                    removed.push(item);
                }
            }
            *bucket = retained;
        }
        instances.retain(|_, bucket| !bucket.is_empty());
        drop(instances);
        for item in &removed {
            item.payload.connection().close().await?;
        }
        Ok(removed.len())
    }
}

pub fn list_runtime_connections() -> Result<Vec<RuntimeInstanceInfo>> {
    crate::runtime::block_async(RuntimeConnectionManager::shared().list_instances())
}

pub fn close_runtime_connection(instance_id: &str) -> Result<bool> {
    crate::runtime::block_async(RuntimeConnectionManager::shared().close_instance(instance_id))
}

pub fn close_runtime_connections_for_config(config_id: &str) -> Result<usize> {
    crate::runtime::block_async(
        RuntimeConnectionManager::shared().close_instances_for_config(config_id),
    )
}

pub fn cleanup_runtime_connections() -> Result<usize> {
    crate::runtime::block_async(RuntimeConnectionManager::shared().cleanup_stale_instances())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_manager::RuntimeConnectionStatus;
    use crate::data_refs::MySqlConfig;
    use sqlx::mysql::MySqlPoolOptions;
    use std::sync::Arc;

    #[test]
    fn reuses_cached_runtime_instance_for_same_config_id() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let manager = RuntimeConnectionManager::new();
            let mysql_ref = Arc::new(MySqlConfig {
                url: Some("mysql://root@localhost/demo".to_string()),
                reconnect_max_attempts: None,
                reconnect_interval_secs: None,
                pool: Some(
                    MySqlPoolOptions::new()
                        .connect_lazy("mysql://root@localhost/demo")
                        .expect("lazy pool"),
                ),
                runtime_handle: None,
            });
            let started_at = Utc::now();
            {
                let mut guard = manager.instances.write().await;
                guard.insert(
                    "cfg-1".to_string(),
                    vec![ManagedConnectionInstance {
                        summary: RuntimeInstanceInfo {
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
                        payload: ConnectionHandle::MySql(Arc::clone(&mysql_ref)),
                        variant: String::new(),
                        config_updated_at: String::new(),
                    }],
                );
            }

            let mut guard = manager.instances.write().await;
            let handle = manager
                .mark_used_and_clone("cfg-1", "", &mut guard)
                .await
                .expect("cached handle");
            drop(guard);

            match handle {
                ConnectionHandle::MySql(found) => {
                    assert!(Arc::ptr_eq(&found, &mysql_ref));
                }
                _ => panic!("expected mysql runtime handle"),
            }

            let guard = manager.instances.read().await;
            assert_eq!(guard.get("cfg-1").map(Vec::len), Some(1));
        });
    }
}
