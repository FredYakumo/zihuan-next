use std::collections::HashMap;

use zihuan_core::error::Result;
use zihuan_graph_engine::{node_output, DataType, DataValue, Node, Port};

use super::support;

pub struct AgentMySqlRefNode {
    id: String,
    name: String,
}

impl AgentMySqlRefNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentMySqlRefNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取 MySQL 连接，并输出 MySqlRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![port! { name = "mysql_ref", ty = MySqlRef, desc = "Agent MySQL 连接引用" },];

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let config = support::current_qq_chat_agent_config()?;
        let mysql_ref = support::build_mysql_ref(config.mysql_connection_id.as_deref())?;
        Ok(HashMap::from([(
            "mysql_ref".to_string(),
            DataValue::MySqlRef(mysql_ref),
        )]))
    }
}
