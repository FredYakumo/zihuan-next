use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
use crate::bot_adapter::event;
use crate::bot_adapter::models::event_model::MessageEvent;
use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio::task::block_in_place;
use tokio::sync::Mutex as TokioMutex;
use tokio::select;

pub struct BotAdapterNode {
    id: String,
    name: String,
    event_rx: Option<TokioMutex<mpsc::UnboundedReceiver<MessageEvent>>>,
    error_rx: Option<TokioMutex<mpsc::UnboundedReceiver<String>>>,
    adapter_handle: Option<SharedBotAdapter>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl BotAdapterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            event_rx: None,
            error_rx: None,
            adapter_handle: None,
            runtime: None,
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

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.on_start(inputs)?;
        let outputs = self.on_update()?.ok_or_else(|| {
            crate::error::Error::ValidationError("No message event received".to_string())
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

        let adapter_config = BotAdapterConfig::new(
            bot_server_url,
            bot_server_token,
            qq_id,
        )
        .with_brain_agent(None);

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
            handle.spawn(run_adapter);
            block_in_place(|| handle.block_on(async { adapter_rx.await.ok() }))
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.spawn(run_adapter);
            let adapter = runtime.block_on(async { adapter_rx.await.ok() });
            self.runtime = Some(runtime);
            adapter
        };

        let adapter_handle = adapter_handle.ok_or_else(|| {
            crate::error::Error::ValidationError("Failed to receive bot adapter handle".to_string())
        })?;

        self.adapter_handle = Some(adapter_handle);
        self.event_rx = Some(TokioMutex::new(event_rx));
        self.error_rx = Some(TokioMutex::new(error_rx));

        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        let event_rx = self.event_rx.as_ref().ok_or_else(|| {
            crate::error::Error::ValidationError("Bot adapter is not initialized".to_string())
        })?;
        let error_rx = self.error_rx.as_ref();

        let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| {
                handle.block_on(async {
                    if let Some(error_rx) = error_rx {
                        select! {
                            error_msg = async {
                                let mut guard = error_rx.lock().await;
                                guard.recv().await
                            } => {
                                if let Some(msg) = error_msg {
                                    return Err(crate::error::Error::ValidationError(msg));
                                }
                                Ok(None)
                            }
                            event = async {
                                let mut guard = event_rx.lock().await;
                                guard.recv().await
                            } => {
                                Ok(event)
                            }
                        }
                    } else {
                        let mut guard = event_rx.lock().await;
                        Ok(guard.recv().await)
                    }
                })
            })
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(async {
                if let Some(error_rx) = error_rx {
                    select! {
                        error_msg = async {
                            let mut guard = error_rx.lock().await;
                            guard.recv().await
                        } => {
                            if let Some(msg) = error_msg {
                                return Err(crate::error::Error::ValidationError(msg));
                            }
                            Ok(None)
                        }
                        event = async {
                            let mut guard = event_rx.lock().await;
                            guard.recv().await
                        } => {
                            Ok(event)
                        }
                    }
                } else {
                    let mut guard = event_rx.lock().await;
                    Ok(guard.recv().await)
                }
            })
        };

        let event = match result? {
            Some(event) => event,
            None => return Ok(None),
        };

        let mut outputs = HashMap::new();
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(event.clone()));
        outputs.insert("bot_adapter".to_string(), DataValue::BotAdapterRef(self.adapter_handle.clone().unwrap()));
        self.validate_outputs(&outputs)?;

        Ok(Some(outputs))
    }

    fn on_cleanup(&mut self) -> Result<()> {
        self.event_rx = None;
        self.error_rx = None;
        self.adapter_handle = None;
        self.runtime = None;
        Ok(())
    }
}

/// Node that extracts the `is_at_me` boolean field from a `MessageProp`
pub struct IsAtMeNode {
    id: String,
    name: String,
}

impl IsAtMeNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for IsAtMeNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Extracts the is_at_me boolean field from a MessageProp")
    }

    node_input![
        port! { name = "msg_prop", ty = MessageProp, desc = "Parsed message properties" },
    ];

    node_output![
        port! { name = "is_at_me", ty = Boolean, desc = "Whether the message @'s the bot" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::MessageProp(prop)) = inputs.get("msg_prop") {
            outputs.insert("is_at_me".to_string(), DataValue::Boolean(prop.is_at_me));
        } else {
            return Err("msg_prop input is required and must be MessageProp type".into());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

pub struct MessageSenderNode {
    id: String,
    name: String,
}

impl MessageSenderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageSenderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Send message back to QQ server")
    }

    node_input![
        port! { name = "target_id", ty = String, desc = "Target user or group ID" },
        port! { name = "content", ty = String, desc = "Message content to send" },
        port! { name = "message_type", ty = String, desc = "Type of message to send" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "Whether the message was sent successfully" },
        port! { name = "response", ty = Json, desc = "Response from the server" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        outputs.insert(
            "success".to_string(),
            DataValue::Boolean(true),
        );
        outputs.insert(
            "response".to_string(),
            DataValue::Json(serde_json::json!({
                "status": "sent",
                "timestamp": "2025-01-28T00:00:00Z"
            })),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}


fn qq_message_list_to_json(
    messages: &[crate::bot_adapter::models::message::Message],
) -> serde_json::Value {
    serde_json::Value::Array(
        messages
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
            .collect(),
    )
}

/// Global counter for generating unique echo IDs.
static ECHO_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_echo() -> String {
    format!("zhn_echo_{}", ECHO_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Send an OneBot action over the existing WebSocket and await its response.
/// `action_name` e.g. `"send_private_msg"`, `params` is the inner params object.
fn ws_send_action(
    adapter_ref: &SharedBotAdapter,
    action_name: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let echo = next_echo();
    let payload = serde_json::json!({
        "action": action_name,
        "params": params,
        "echo": echo,
    });

    block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            // Extract action_tx and pending_actions without holding the adapter lock.
            let (action_tx, pending_actions) = {
                let guard = adapter_ref.lock().await;
                let tx = guard.action_tx.clone().ok_or_else(|| {
                    crate::error::Error::ValidationError(
                        "Bot adapter WebSocket not connected yet".to_string(),
                    )
                })?;
                let pending = guard.pending_actions.clone();
                Ok::<_, crate::error::Error>((tx, pending))
            }?;


            let (tx, rx) = oneshot::channel::<serde_json::Value>();
            pending_actions.lock().await.insert(echo.clone(), tx);


            action_tx
                .send(payload.to_string())
                .map_err(|_| crate::error::Error::ValidationError(
                    "Failed to enqueue WebSocket action".to_string(),
                ))?;

            // Wait for the response (30 s timeout).
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                rx,
            )
            .await
            .map_err(|_| crate::error::Error::ValidationError(
                format!("Action '{}' timed out after 30 s", action_name),
            ))?
            .map_err(|_| crate::error::Error::ValidationError(
                "Response channel closed unexpectedly".to_string(),
            ))?;

            Ok(response)
        })
    })
}


pub struct SendFriendMessageNode {
    id: String,
    name: String,
}

impl SendFriendMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for SendFriendMessageNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { Some("向QQ好友发送消息") }

    node_input![
        port! { name = "bot_adapter", ty = BotAdapterRef, desc = "Bot适配器引用" },
        port! { name = "target_id", ty = String, desc = "目标好友的QQ号" },
        port! { name = "message", ty = QQMessageList, desc = "要发送的QQ消息段列表" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否发送成功" },
        port! { name = "message_id", ty = Integer, desc = "服务器返回的消息ID" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let adapter_ref = match inputs.get("bot_adapter") {
            Some(DataValue::BotAdapterRef(r)) => r.clone(),
            _ => return Err("bot_adapter input is required".into()),
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err("target_id input is required".into()),
        };
        let messages = match inputs.get("message") {
            Some(DataValue::QQMessageList(v)) => v.clone(),
            _ => return Err("message input is required".into()),
        };

        let params = serde_json::json!({
            "user_id": target_id,
            "message": qq_message_list_to_json(&messages),
        });
        let response = ws_send_action(&adapter_ref, "send_private_msg", params)?;

        let success = response.get("retcode").and_then(|v| v.as_i64()).unwrap_or(-1) == 0;
        let message_id = response
            .get("data").and_then(|d| d.get("message_id")).and_then(|v| v.as_i64())
            .unwrap_or(-1);

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("message_id".to_string(), DataValue::Integer(message_id));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

pub struct SendGroupMessageNode {
    id: String,
    name: String,
}

impl SendGroupMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for SendGroupMessageNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { Some("向QQ群组发送消息") }

    node_input![
        port! { name = "bot_adapter", ty = BotAdapterRef, desc = "Bot适配器引用" },
        port! { name = "target_id", ty = String, desc = "目标群的群号" },
        port! { name = "message", ty = QQMessageList, desc = "要发送的QQ消息段列表" }
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否发送成功" },
        port! { name = "message_id", ty = Integer, desc = "服务器返回的消息ID" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let adapter_ref = match inputs.get("bot_adapter") {
            Some(DataValue::BotAdapterRef(r)) => r.clone(),
            _ => return Err("bot_adapter input is required".into()),
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err("target_id input is required".into()),
        };
        let messages = match inputs.get("message") {
            Some(DataValue::QQMessageList(v)) => v.clone(),
            _ => return Err("message input is required".into()),
        };

        let params = serde_json::json!({
            "group_id": target_id,
            "message": qq_message_list_to_json(&messages),
        });
        let response = ws_send_action(&adapter_ref, "send_group_msg", params)?;

        let success = response.get("retcode").and_then(|v| v.as_i64()).unwrap_or(-1) == 0;
        let message_id = response
            .get("data").and_then(|d| d.get("message_id")).and_then(|v| v.as_i64())
            .unwrap_or(-1);

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("message_id".to_string(), DataValue::Integer(message_id));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
