use crate::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
use crate::event;
use crate::models::event_model::MessageEvent;
use log::{error, info};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::select;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::{mpsc, oneshot};
use tokio::task::block_in_place;
use zihuan_core::error::Result;
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};

pub struct BotAdapterNode {
    id: String,
    name: String,
    event_rx: Option<TokioMutex<mpsc::UnboundedReceiver<MessageEvent>>>,
    error_rx: Option<TokioMutex<mpsc::UnboundedReceiver<String>>>,
    adapter_handle: Option<SharedBotAdapter>,
    adapter_task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Runtime>,
    stop_flag: Option<Arc<AtomicBool>>,
}

impl BotAdapterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            event_rx: None,
            error_rx: None,
            adapter_handle: None,
            adapter_task: None,
            runtime: None,
            stop_flag: None,
        }
    }
}

impl Node for BotAdapterNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn node_type(&self) -> NodeType {
        NodeType::EventProducer
    }

    fn description(&self) -> Option<&str> {
        Some("QQ Bot Adapter - receives messages from QQ server")
    }

    node_input![
        port! { name = "qq_id", ty = String, desc = "QQ ID to login" },
        port! { name = "bot_server_url", ty = String, desc = "Bot服务器WebSocket地址" },
        port! { name = "bot_server_token", ty = Password, desc = "Bot服务器连接令牌", optional },
    ];

    node_output![
        port! { name = "message_event", ty = MessageEvent, desc = "Raw message event from QQ server" },
        port! { name = "bot_adapter", ty = BotAdapterRef, desc = "Shared reference to the bot adapter instance" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.on_start(inputs)?;
        let outputs = self.on_update()?.ok_or_else(|| {
            zihuan_core::error::Error::ValidationError("No message event received".to_string())
        })?;
        Ok(outputs)
    }

    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> {
        if self.event_rx.is_some() {
            return Ok(());
        }

        self.validate_inputs(&inputs)?;

        let qq_id = inputs
            .get("qq_id")
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| std::env::var("QQ_ID").unwrap_or_default());

        let bot_server_url = inputs
            .get("bot_server_url")
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| {
                std::env::var("BOT_SERVER_URL")
                    .unwrap_or_else(|_| "ws://localhost:3001".to_string())
            });

        let bot_server_token = inputs
            .get("bot_server_token")
            .and_then(|value| match value {
                DataValue::Password(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| std::env::var("BOT_SERVER_TOKEN").unwrap_or_default());

        let adapter_config =
            BotAdapterConfig::new(bot_server_url, bot_server_token, qq_id).with_brain_agent(None);

        let (event_tx, event_rx) = mpsc::unbounded_channel::<MessageEvent>();
        let (adapter_tx, adapter_rx) = oneshot::channel();
        let (error_tx, error_rx) = mpsc::unbounded_channel::<String>();
        let handler: event::EventHandler = Arc::new(move |event| {
            let event_tx = event_tx.clone();
            Box::pin(async move {
                let _ = event_tx.send(event.clone());
            })
        });

        let run_adapter = async move {
            let mut adapter = BotAdapter::new(adapter_config).await;
            adapter.register_event_handler(handler);
            let adapter = adapter.into_shared();
            let _ = adapter_tx.send(adapter.clone());
            info!("Bot adapter initialized, connecting to server...");
            if let Err(e) = BotAdapter::start(adapter).await {
                error!("Bot adapter error: {}", e);
                let _ = error_tx.send(format!("Bot adapter connection error: {}", e));
            }
        };

        let adapter_handle = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let task = handle.spawn(run_adapter);
            self.adapter_task = Some(task);
            block_in_place(|| handle.block_on(async { adapter_rx.await.ok() }))
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.spawn(run_adapter);
            let adapter = runtime.block_on(async { adapter_rx.await.ok() });
            self.runtime = Some(runtime);
            adapter
        };

        let adapter_handle = adapter_handle.ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(
                "Failed to receive bot adapter handle".to_string(),
            )
        })?;

        self.adapter_handle = Some(adapter_handle);
        self.event_rx = Some(TokioMutex::new(event_rx));
        self.error_rx = Some(TokioMutex::new(error_rx));

        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        let event_rx = self.event_rx.as_ref().ok_or_else(|| {
            zihuan_core::error::Error::ValidationError("Bot adapter is not initialized".to_string())
        })?;
        let error_rx = self.error_rx.as_ref();
        let stop_flag = self.stop_flag.clone();

        let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| {
                handle.block_on(async {
                    let stop_flag = stop_flag.clone();
                    if let Some(error_rx) = error_rx {
                        select! {
                            error_msg = async {
                                let mut guard = error_rx.lock().await;
                                guard.recv().await
                            } => {
                                if let Some(msg) = error_msg {
                                    return Err(zihuan_core::error::Error::ValidationError(msg));
                                }
                                Ok(None)
                            }
                            event = async {
                                let mut guard = event_rx.lock().await;
                                guard.recv().await
                            } => {
                                Ok(event)
                            }
                            _ = async move {
                                loop {
                                    if let Some(ref flag) = stop_flag {
                                        if flag.load(Ordering::Relaxed) { return; }
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            } => {
                                Ok(None)
                            }
                        }
                    } else {
                        select! {
                            event = async {
                                let mut guard = event_rx.lock().await;
                                guard.recv().await
                            } => {
                                Ok(event)
                            }
                            _ = async move {
                                loop {
                                    if let Some(ref flag) = stop_flag {
                                        if flag.load(Ordering::Relaxed) { return; }
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            } => {
                                Ok(None)
                            }
                        }
                    }
                })
            })
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(async {
                let stop_flag = stop_flag.clone();
                if let Some(error_rx) = error_rx {
                    select! {
                        error_msg = async {
                            let mut guard = error_rx.lock().await;
                            guard.recv().await
                        } => {
                            if let Some(msg) = error_msg {
                                return Err(zihuan_core::error::Error::ValidationError(msg));
                            }
                            Ok(None)
                        }
                        event = async {
                            let mut guard = event_rx.lock().await;
                            guard.recv().await
                        } => {
                            Ok(event)
                        }
                        _ = async move {
                            loop {
                                if let Some(ref flag) = stop_flag {
                                    if flag.load(Ordering::Relaxed) { return; }
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        } => {
                            Ok(None)
                        }
                    }
                } else {
                    select! {
                        event = async {
                            let mut guard = event_rx.lock().await;
                            guard.recv().await
                        } => {
                            Ok(event)
                        }
                        _ = async move {
                            loop {
                                if let Some(ref flag) = stop_flag {
                                    if flag.load(Ordering::Relaxed) { return; }
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        } => {
                            Ok(None)
                        }
                    }
                }
            })
        };

        let event = match result? {
            Some(event) => event,
            None => return Ok(None),
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "message_event".to_string(),
            DataValue::MessageEvent(event.clone()),
        );
        outputs.insert(
            "bot_adapter".to_string(),
            DataValue::BotAdapterRef(
                self.adapter_handle.clone().unwrap() as zihuan_bot_types::BotAdapterHandle
            ),
        );
        self.validate_outputs(&outputs)?;

        Ok(Some(outputs))
    }

    fn on_cleanup(&mut self) -> Result<()> {
        if let Some(task) = self.adapter_task.take() {
            task.abort();
        }
        self.event_rx = None;
        self.error_rx = None;
        self.adapter_handle = None;
        self.runtime = None;
        self.stop_flag = None;
        Ok(())
    }

    fn set_stop_flag(&mut self, stop_flag: Arc<AtomicBool>) {
        self.stop_flag = Some(stop_flag);
    }
}
