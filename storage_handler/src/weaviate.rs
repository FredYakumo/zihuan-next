use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::database::weaviate::WeaviateRef;
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::RuntimeStorageConnectionManager;

const CONFIG_ID_FIELD: &str = "config_id";
const LEGACY_CONNECTION_ID_FIELD: &str = "connection_id";

pub fn build_weaviate_ref(
    base_url: &str,
    class_name: &str,
    _image_collection: bool,
) -> Result<Arc<WeaviateRef>> {
    let mut node: Box<dyn Node> = Box::new(zihuan_graph_engine::database::WeaviateNode::new(
        "__storage_handler__",
        "__storage_handler__",
    ));

    let outputs = node.execute(HashMap::from([
        (
            "base_url".to_string(),
            DataValue::String(base_url.to_string()),
        ),
        (
            "class_name".to_string(),
            DataValue::String(class_name.to_string()),
        ),
    ]))?;

    match outputs.get("weaviate_ref") {
        Some(DataValue::WeaviateRef(reference)) => Ok(reference.clone()),
        _ => Err(Error::StringError(
            "weaviate node did not return weaviate_ref".to_string(),
        )),
    }
}

pub struct WeaviateNode {
    id: String,
    name: String,
    config_id: Option<String>,
}

impl WeaviateNode {
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
        .with_connection_kind("weaviate")
        .with_description("选择系统中的 Weaviate 连接配置")
    }
}

impl Node for WeaviateNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Weaviate 向量数据库配置 - 从系统连接中选择并输出 WeaviateRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("weaviate_ref", DataType::WeaviateRef)
            .with_description("Weaviate 数据库引用")]
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
        let weaviate_ref = zihuan_core::runtime::block_async(
            RuntimeStorageConnectionManager::shared().get_or_create_weaviate_ref(config_id),
        )?;

        Ok(HashMap::from([(
            "weaviate_ref".to_string(),
            DataValue::WeaviateRef(weaviate_ref),
        )]))
    }
}
