use std::collections::HashMap;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::active_adapter_manager::ActiveAdapterManager;

const CONFIG_ID_FIELD: &str = "config_id";
const LEGACY_CONNECTION_ID_FIELD: &str = "connection_id";

pub struct ImsBotAdapterProviderNode {
    id: String,
    name: String,
    config_id: Option<String>,
}

impl ImsBotAdapterProviderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            config_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(
            CONFIG_ID_FIELD,
            DataType::String,
            NodeConfigWidget::ConnectionSelect,
        )
        .with_connection_kind("bot_adapter")
        .with_description("选择系统中的 IMS Bot Adapter 连接配置")
    }
}

impl Node for ImsBotAdapterProviderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从系统连接中选择 IMS Bot Adapter 并输出 BotAdapterRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("ims_bot_adapter", DataType::BotAdapterRef)
            .with_description("IMS Bot Adapter 引用")]
    }

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::connection_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.config_id = inline_values
            .get(CONFIG_ID_FIELD)
            .or_else(|| inline_values.get(LEGACY_CONNECTION_ID_FIELD))
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let config_id = self
            .config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("config_id is required".to_string()))?;
        let handle = zihuan_core::runtime::block_async(
            ActiveAdapterManager::shared().get_active_bot_adapter_handle(config_id),
        )?;

        Ok(HashMap::from([(
            "ims_bot_adapter".to_string(),
            DataValue::BotAdapterRef(handle),
        )]))
    }
}
