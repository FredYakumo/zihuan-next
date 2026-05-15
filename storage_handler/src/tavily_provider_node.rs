use std::collections::HashMap;

use crate::load_connections;
use crate::resource_resolver::build_tavily_ref;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{
    node_output, DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port,
};

const CONFIG_ID_FIELD: &str = "config_id";
const LEGACY_CONNECTION_ID_FIELD: &str = "connection_id";

pub struct TavilyProviderNode {
    id: String,
    name: String,
    config_id: Option<String>,
}

impl TavilyProviderNode {
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
        .with_connection_kind("tavily")
        .with_description("选择系统中的 Tavily 连接配置")
    }

    fn selected_config_id(&self) -> Result<&str> {
        self.config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("config_id is required".to_string()))
    }
}

impl Node for TavilyProviderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Tavily 搜索配置 - 从系统连接中选择并输出 TavilyRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![port! { name = "tavily_ref", ty = DataType::TavilyRef, desc = "Tavily 搜索引用" },];

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
        let config_id = self.selected_config_id()?;
        let tavily_ref = build_tavily_ref(Some(config_id), &load_connections()?)?
            .ok_or_else(|| Error::ValidationError("config_id is required".to_string()))?;

        Ok(HashMap::from([(
            "tavily_ref".to_string(),
            DataValue::TavilyRef(tavily_ref),
        )]))
    }
}
