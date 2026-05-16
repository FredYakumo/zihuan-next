use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::error::{Error, Result};
use zihuan_core::weaviate::{WeaviateCollectionSchema, WeaviateRef};
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::RuntimeStorageConnectionManager;

const CONFIG_ID_FIELD: &str = "config_id";
const LEGACY_CONNECTION_ID_FIELD: &str = "connection_id";

pub fn build_weaviate_ref(
    base_url: &str,
    class_name: &str,
    username: Option<String>,
    password: Option<String>,
    api_key: Option<String>,
    collection_schema: WeaviateCollectionSchema,
) -> Result<Arc<WeaviateRef>> {
    let weaviate_ref = Arc::new(WeaviateRef::new(
        base_url,
        class_name,
        username,
        password,
        api_key,
        std::time::Duration::from_secs(30),
    )?);
    if !weaviate_ref.ready()? {
        return Err(Error::StringError(
            "Weaviate is reachable but not ready yet".to_string(),
        ));
    }
    weaviate_ref.ensure_collection_schema(collection_schema, true)?;
    Ok(weaviate_ref)
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
