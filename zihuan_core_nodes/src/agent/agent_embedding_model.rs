use std::collections::HashMap;

use zihuan_core::agent_config::current_qq_chat_agent_config;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_output, DataType, DataValue, Node, Port};

use zihuan_llm::agent_config_support::build_embedding_from_ref_id;

pub struct AgentEmbeddingModelNode {
    id: String,
    name: String,
}

impl AgentEmbeddingModelNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for AgentEmbeddingModelNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取文本向量模型，并输出 EmbeddingModel 引用")
    }
    fn input_ports(&self) -> Vec<Port> { Vec::new() }

    node_output![
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "Agent 文本向量模型引用" },
    ];

    fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let config = current_qq_chat_agent_config()?;
        let model = build_embedding_from_ref_id(config.embedding_model_ref_id.as_deref())?;
        Ok(HashMap::from([("embedding_model".to_string(), DataValue::EmbeddingModel(model))]))
    }
}
