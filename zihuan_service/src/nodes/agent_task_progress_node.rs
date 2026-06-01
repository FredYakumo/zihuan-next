use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct AgentTaskProgressNode {
    id: String,
    name: String,
}

impl AgentTaskProgressNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AgentTaskProgressNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("向任务写入一条进度消息")
    }

    node_input![
        port! { name = "task_id", ty = String, desc = "要更新的任务 ID" },
        port! { name = "message", ty = String, desc = "要追加的进度消息" },
    ];

    node_output![port! { name = "ok", ty = Boolean, desc = "是否成功写入进度" },];

    fn execute(
        &mut self,
        inputs: zihuan_graph_engine::NodeInputFlow,
    ) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let task_id = inputs
            .get("task_id")
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.trim().to_string()),
                _ => None,
            })
            .unwrap_or_default();
        let message = inputs
            .get("message")
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.trim().to_string()),
                _ => None,
            })
            .unwrap_or_default();

        let ok = if task_id.is_empty() || message.is_empty() {
            false
        } else if let Some(runtime) = crate::command::global_task_runtime() {
            runtime.append_task_progress(&task_id, message);
            true
        } else {
            false
        };

        zihuan_graph_engine::return_with_node_output![self; "ok" => DataValue::Boolean(ok)]
    }
}
