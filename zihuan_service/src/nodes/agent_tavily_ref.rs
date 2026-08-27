use std::collections::HashMap;

use zihuan_core::storage::{load_connections, resource_resolver};
use zihuan_core::agent::runtime_context::current_qq_chat_agent_service_config;
use zihuan_core::error::Result;
use zihuan_core::graph::{node_output, DataType, DataValue, Node, NodeOutputFlow, Port};

pub struct AgentTavilyRefNode {
    id: String,
    name: String,
}

impl AgentTavilyRefNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentTavilyRefNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取 Web Search Engine 连接，并输出 WebSearchEngineRef")
    }
    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![
        port! { name = "web_search_engine_ref", ty = WebSearchEngineRef, desc = "Agent Web Search Engine 搜索引用" },
    ];

    fn execute(&mut self, _inputs: zihuan_core::graph::NodeInputFlow) -> Result<zihuan_core::graph::NodeOutputFlow> {
        let config = current_qq_chat_agent_service_config()?;
        let web_search_engine_connection_id = config.web_search_engine_connection_id.trim();
        if web_search_engine_connection_id.is_empty() {
            return Err(zihuan_core::error::Error::ValidationError(
                "web_search_engine_connection_id is required".to_string(),
            ));
        }
        let connections = load_connections()?;
        let web_search_engine_ref =
            resource_resolver::build_web_search_engine_ref(Some(web_search_engine_connection_id), &connections)?
                .ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(
                        "web_search_engine_connection_id is required".to_string(),
                    )
                })?;
        zihuan_core::graph::return_with_node_output![self;
            "web_search_engine_ref" => DataValue::WebSearchEngineRef(web_search_engine_ref),
        ]
    }
}
