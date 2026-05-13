use std::collections::HashMap;

use storage_handler::{load_connections, resource_resolver};
use zihuan_core::agent_config::current_qq_chat_agent_config;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_output, DataValue, DataType, Node, Port};

pub struct AgentTavilyRefNode {
    id: String,
    name: String,
}

impl AgentTavilyRefNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for AgentTavilyRefNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取 Tavily 连接，并输出 TavilyRef")
    }
    fn input_ports(&self) -> Vec<Port> { Vec::new() }

    node_output![port! { name = "tavily_ref", ty = TavilyRef, desc = "Agent Tavily 搜索引用" },];

    fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let config = current_qq_chat_agent_config()?;
        let tavily_connection_id = config.tavily_connection_id.trim();
        if tavily_connection_id.is_empty() {
            return Err(zihuan_core::error::Error::ValidationError("tavily_connection_id is required".to_string()));
        }
        let connections = load_connections()?;
        let tavily_ref = resource_resolver::build_tavily_ref(Some(tavily_connection_id), &connections)?
            .ok_or_else(|| zihuan_core::error::Error::ValidationError("tavily_connection_id is required".to_string()))?;
        Ok(HashMap::from([("tavily_ref".to_string(), DataValue::TavilyRef(tavily_ref))]))
    }
}
