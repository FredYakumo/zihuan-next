//! Owns a bot transport's reconnect and heartbeat tasks.
use super::adapter::{BotAdapter, SharedBotAdapter};
use super::ws_action::ws_send_action_async;
use crate::connection::Connection;
use crate::error::Result;
use log::{error, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

pub const BOT_ADAPTER_HEARTBEAT_INTERVAL_SECS: u64 = 30;

pub struct BotConnection {
    pub adapter: SharedBotAdapter,
    config_id: String,
    name: String,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    closed: AtomicBool,
    connected: Arc<AtomicBool>,
}

impl BotConnection {
    pub async fn new(config_id: String, name: String, adapter: SharedBotAdapter) -> Self {
        let connected = adapter.lock().await.connected.clone();
        Self {
            config_id,
            name,
            adapter,
            tasks: Mutex::new(Vec::new()),
            closed: AtomicBool::new(true),
            connected,
        }
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
                let pending = {
                    let mut guard = adapter.lock().await;
                    guard.action_tx = None;
                    guard.connected.store(false, Ordering::Release);
                    guard.pending_actions.clone()
                };
                pending.lock().await.clear();
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
                match ws_send_action_async(&adapter, "get_login_info", serde_json::json!({})).await
                {
                    Ok(_) => {
                        log::debug!(
                            "[active_adapter_manager] heartbeat ok for '{}' (instance_id={})",
                            connection_name,
                            instance_id
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
}

#[async_trait::async_trait]
impl Connection for BotConnection {
    async fn connect(&mut self) -> Result<()> {
        if !self.is_closed() {
            return Ok(());
        }
        self.closed.store(false, Ordering::Release);
        let tasks = self.tasks.get_mut().unwrap_or_else(|e| e.into_inner());
        tasks.push(Self::spawn_keepalive_loop(
            self.config_id.clone(),
            self.name.clone(),
            self.adapter.clone(),
        ));
        tasks.push(Self::spawn_heartbeat_loop(
            self.config_id.clone(),
            self.name.clone(),
            self.adapter.clone(),
        ));
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        if self.is_closed() {
            return Err(crate::string_error!("Bot connection is closed"));
        }
        ws_send_action_async(&self.adapter, "get_login_info", serde_json::json!({}))
            .await
            .map(|_| ())
    }

    async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        self.connected.store(false, Ordering::Release);
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap_or_else(|e| e.into_inner()));
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
        let pending = {
            let mut adapter = self.adapter.lock().await;
            adapter.action_tx = None;
            adapter.pending_actions.clone()
        };
        pending.lock().await.clear();
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn status(&self) -> crate::connection_manager::RuntimeConnectionStatus {
        use crate::connection_manager::RuntimeConnectionStatus;
        if self.is_closed() {
            RuntimeConnectionStatus::Closed
        } else if self.connected.load(Ordering::Acquire) {
            RuntimeConnectionStatus::Running
        } else {
            RuntimeConnectionStatus::Connecting
        }
    }

    fn keep_alive(&self) -> bool {
        true
    }
    fn heartbeat_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(BOT_ADAPTER_HEARTBEAT_INTERVAL_SECS))
    }
}

impl Drop for BotConnection {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::Release);
        for task in self.tasks.get_mut().unwrap_or_else(|e| e.into_inner()).drain(..) {
            task.abort();
        }
    }
}
