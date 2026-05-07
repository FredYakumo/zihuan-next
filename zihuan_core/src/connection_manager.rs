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
pub struct RuntimeConnectionInstanceSummary {
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

#[async_trait]
pub trait ConnectionManager: Send + Sync {
    type Handle: Clone + Send + Sync + 'static;

    async fn get_or_create(&self, config_id: &str) -> Result<Self::Handle>;

    async fn list_instances(&self) -> Result<Vec<RuntimeConnectionInstanceSummary>>;

    async fn close_instance(&self, instance_id: &str) -> Result<bool>;

    async fn close_instances_for_config(&self, config_id: &str) -> Result<usize>;

    async fn cleanup_stale_instances(&self) -> Result<usize>;
}
