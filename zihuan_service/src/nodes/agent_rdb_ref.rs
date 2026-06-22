use std::collections::HashMap;

use storage_handler::RuntimeStorageConnectionManager;
use zihuan_core::agent_config::current_qq_chat_agent_service_config;
use zihuan_core::error::Result;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_graph_engine::{node_output, DataType, DataValue, Node, Port};

pub struct AgentRdbRefNode {
    id: String,
    name: String,
}

impl AgentRdbRefNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentRdbRefNode {
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

    node_output![port! { name = "rdb_ref", ty = RdbRef, desc = "Agent 关系数据库连接引用" },];

    fn execute(&mut self, _inputs: zihuan_graph_engine::NodeInputFlow) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        let config = current_qq_chat_agent_service_config()?;
        let rdb_connection_id = config.resolved_rdb_id();
        let rdb_connection_id = rdb_connection_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| zihuan_core::error::Error::ValidationError("rdb_connection_id is required".to_string()))?;
        let rdb_ref = zihuan_core::runtime::block_async(
            RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(rdb_connection_id),
        )?;
        zihuan_graph_engine::return_with_node_output![self;
            "rdb_ref" => DataValue::RdbRef(RelationalDbConnection::MySql(rdb_ref)),
        ]
    }
}
