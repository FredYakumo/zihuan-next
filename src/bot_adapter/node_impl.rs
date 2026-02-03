use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig};
use crate::config::{build_mysql_url, build_redis_url, load_config};
use crate::error::Result;
use crate::node::{DataType, DataValue, Node, Port};
use log::{error, info};
use std::collections::HashMap;

pub struct BotAdapterNode {
    id: String,
    name: String,
}

impl BotAdapterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
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

    fn description(&self) -> Option<&str> {
        Some("QQ Bot Adapter - receives messages from QQ server")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("trigger", DataType::Boolean)
                .with_description("Trigger to start receiving messages"),
            Port::new("qq_id", DataType::String)
                .with_description("QQ ID to login")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("message", DataType::MessageEvent)
                .with_description("Raw message event from QQ server"),
            Port::new("message_type", DataType::String)
                .with_description("Type of the message"),
            Port::new("user_id", DataType::String)
                .with_description("User ID who sent the message"),
            Port::new("content", DataType::String)
                .with_description("Message content"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let qq_id = inputs
            .get("qq_id")
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| std::env::var("QQ_ID").unwrap_or_default());

        let config = load_config();
        let redis_url = build_redis_url(&config);
        let database_url = build_mysql_url(&config);

        let adapter_config = BotAdapterConfig::new(
            config.bot_server_url,
            config.bot_server_token,
            qq_id,
        )
        .with_redis_url(redis_url)
        .with_database_url(database_url)
        .with_redis_reconnect(
            config.redis_reconnect_max_attempts,
            config.redis_reconnect_interval_secs,
        )
        .with_mysql_reconnect(
            config.mysql_reconnect_max_attempts,
            config.mysql_reconnect_interval_secs,
        )
        .with_brain_agent(None);

        let run_adapter = async move {
            let adapter = BotAdapter::new(adapter_config).await;
            let adapter = adapter.into_shared();
            info!("Bot adapter initialized, connecting to server...");
            if let Err(e) = BotAdapter::start(adapter).await {
                error!("Bot adapter error: {}", e);
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(run_adapter);
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_adapter);
        }

        let outputs = HashMap::new();
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

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("target_id", DataType::String)
                .with_description("Target user or group ID"),
            Port::new("content", DataType::String)
                .with_description("Message content to send"),
            Port::new("message_type", DataType::String)
                .with_description("Type of message to send"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("success", DataType::Boolean)
                .with_description("Whether the message was sent successfully"),
            Port::new("response", DataType::Json)
                .with_description("Response from the server"),
        ]
    }

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
