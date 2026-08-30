use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeConnectionStatus {
    Running,
    Idle,
    Closing,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInstanceInfo {
    pub instance_id: String,
    pub config_id: String,
    pub name: String,
    pub kind: String,
    pub keep_alive: bool,
    pub heartbeat_interval_secs: Option<u64>,
    pub started_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub status: RuntimeConnectionStatus,
}

/// Defines the common runtime lifecycle contract for ZiHuan Next configuration managers.
///
/// Each active configuration is represented by a [`RuntimeInstanceInfo`]
/// containing its runtime instance ID, source configuration ID, display name, kind,
/// keep-alive and heartbeat settings, start and last-use timestamps, and current status.
///
/// Implementations must provide the following capabilities:
/// - Reuse an existing runtime handle for a configuration or create one on demand.
/// - List the summaries of all active runtime instances.
/// - Close one instance or every instance created from a configuration.
/// - Remove instances that have become idle, disabled, or otherwise stale.
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    type Handle: Clone + Send + Sync + 'static;

    async fn get_or_create(&self, config_id: &str) -> Result<Self::Handle>;

    async fn list_instances(&self) -> Result<Vec<RuntimeInstanceInfo>>;

    async fn close_instance(&self, instance_id: &str) -> Result<bool>;

    async fn close_instances_for_config(&self, config_id: &str) -> Result<usize>;

    async fn cleanup_stale_instances(&self) -> Result<usize>;
}
