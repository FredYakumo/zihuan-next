use std::collections::HashMap;

use zihuan_core::error::Result;
use zihuan_core::task_context::current_task_id;
use zihuan_graph_engine::{node_output, DataType, DataValue, Node, Port};

pub struct AgentToolTaskNode {
    id: String,
    name: String,
}

impl AgentToolTaskNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentToolTaskNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("读取当前 Agent 工具调用关联的任务 ID")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![
        port! { name = "task_id", ty = String, desc = "当前工具调用绑定的任务 ID；若不存在则为空字符串" },
        port! { name = "has_task", ty = Boolean, desc = "当前是否存在绑定的任务" },
    ];

    fn execute(&mut self, _inputs: zihuan_graph_engine::NodeInputFlow) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        let task_id = current_task_id().unwrap_or_default();
        let has_task = !task_id.trim().is_empty();

        zihuan_graph_engine::return_with_node_output![self;
            "task_id" => DataValue::String(task_id.clone()),
            "has_task" => DataValue::Boolean(has_task)
        ]
    }
}
