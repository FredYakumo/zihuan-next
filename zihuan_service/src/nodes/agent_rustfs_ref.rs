use std::collections::HashMap;

use storage_handler::RuntimeStorageConnectionManager;
use zihuan_core::agent_config::current_qq_chat_agent_config;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_output, DataValue, DataType, Node, Port};

pub struct AgentRustfsRefNode {
    id: String,
    name: String,
}

impl AgentRustfsRefNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentRustfsRefNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取 RustFS 连接，并输出 S3Ref")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![port! { name = "s3_ref", ty = S3Ref, desc = "Agent RustFS 对象存储引用" },];

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let config = current_qq_chat_agent_config()?;
        let rustfs_connection_id = config.rustfs_connection_id.as_deref();
        let rustfs_connection_id = rustfs_connection_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(
                    "rustfs_connection_id is required".to_string(),
                )
            })?;
        let s3_ref = zihuan_core::runtime::block_async(
            RuntimeStorageConnectionManager::shared().get_or_create_s3_ref(rustfs_connection_id),
        )?;
        Ok(HashMap::from([(
            "s3_ref".to_string(),
            DataValue::S3Ref(s3_ref),
        )]))
    }
}
