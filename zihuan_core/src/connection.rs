//! Lifecycle of one concrete connection. Managers own configuration lookup and reuse;
//! implementations own their resources, probes and transport tasks.
use async_trait::async_trait;

use crate::connection_manager::RuntimeConnectionStatus;
use crate::error::Result;
use std::time::Duration;

#[async_trait]
pub trait Connection: Send + Sync {
    /// Open or initialize the resource. Repeated calls on an open connection are safe.
    async fn connect(&mut self) -> Result<()>;

    /// Probe the existing resource without opening a replacement connection.
    async fn health_check(&self) -> Result<()>;

    /// Prevent further use and release owned resources/tasks. Must be idempotent.
    /// Operations already in flight may finish; no automatic reconnect may survive close.
    async fn close(&self) -> Result<()>;

    fn is_closed(&self) -> bool;

    /// Local lifecycle state; listing instances must not create/probe a connection.
    fn status(&self) -> RuntimeConnectionStatus {
        if self.is_closed() {
            RuntimeConnectionStatus::Closed
        } else {
            RuntimeConnectionStatus::Running
        }
    }

    fn keep_alive(&self) -> bool {
        false
    }

    fn heartbeat_interval(&self) -> Option<Duration> {
        None
    }

    /// Reopening is explicit and needs exclusive ownership of the connection.
    async fn reconnect(&mut self) -> Result<()> {
        self.close().await?;
        self.connect().await
    }
}
