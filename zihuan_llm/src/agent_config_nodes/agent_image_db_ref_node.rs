use std::collections::HashMap;

use zihuan_core::error::Result;
use zihuan_graph_engine::{node_output, DataType, DataValue, Node, Port};

use super::support;

pub struct AgentImageDbRefNode {
    id: String,
    name: String,
}

impl AgentImageDbRefNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentImageDbRefNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取图片向量库连接，并输出 WeaviateRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![port! { name = "weaviate_ref", ty = WeaviateRef, desc = "Agent 图片向量库引用" },];

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let config = support::current_qq_chat_agent_config()?;
        let weaviate_ref =
            support::build_image_db_ref(config.weaviate_image_connection_id.as_deref())?;
        support::ensure_image_schema(&weaviate_ref)?;
        Ok(HashMap::from([(
            "weaviate_ref".to_string(),
            DataValue::WeaviateRef(weaviate_ref),
        )]))
    }
}
