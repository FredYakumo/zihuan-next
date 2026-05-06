use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::database::weaviate::WeaviateRef;
use zihuan_graph_engine::database::{
    WeaviateImageCollectionNode as LegacyWeaviateImageCollectionNode,
    WeaviateNode as LegacyWeaviateNode,
};
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::{load_connections, resource_resolver};

const CONNECTION_ID_FIELD: &str = "connection_id";

pub fn build_weaviate_ref(
    base_url: &str,
    class_name: &str,
    image_collection: bool,
) -> Result<Arc<WeaviateRef>> {
    let mut node: Box<dyn Node> = if image_collection {
        Box::new(LegacyWeaviateImageCollectionNode::new(
            "__storage_handler__",
            "__storage_handler__",
        ))
    } else {
        Box::new(LegacyWeaviateNode::new(
            "__storage_handler__",
            "__storage_handler__",
        ))
    };

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
    connection_id: Option<String>,
}

impl WeaviateNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            connection_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(
            CONNECTION_ID_FIELD,
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
        Some("Weaviate 消息 collection 配置 - 从系统连接中选择并输出 WeaviateRef")
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
        self.connection_id = inline_values
            .get(CONNECTION_ID_FIELD)
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
        let connection_id = self
            .connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("connection_id is required".to_string()))?;
        let connections = load_connections()?;
        let weaviate_ref =
            resource_resolver::build_weaviate_ref(Some(connection_id), &connections, false)?
                .ok_or_else(|| Error::ValidationError("connection_id is required".to_string()))?;

        Ok(HashMap::from([(
            "weaviate_ref".to_string(),
            DataValue::WeaviateRef(weaviate_ref),
        )]))
    }
}
