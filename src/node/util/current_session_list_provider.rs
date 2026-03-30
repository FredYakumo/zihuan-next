use crate::error::Result;
use crate::node::data_value::CurrentSessionRegistryRef;
use crate::node::{node_output, DataType, DataValue, Node, NodeType, Port};
use std::collections::HashMap;
use std::sync::Arc;

pub struct CurrentSessionListProviderNode {
    id: String,
    name: String,
    session_registry_ref: Arc<CurrentSessionRegistryRef>,
    run_initialized: bool,
}

impl CurrentSessionListProviderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            session_registry_ref: Arc::new(CurrentSessionRegistryRef::new(id.clone())),
            id,
            name: name.into(),
            run_initialized: false,
        }
    }

    fn initialize_run(&mut self) {
        if self.run_initialized {
            return;
        }

        self.session_registry_ref.clear();
        self.run_initialized = true;
    }
}

impl Node for CurrentSessionListProviderNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("创建单次节点图运行期的当前会话列表引用，供图内 sender 级互斥控制使用")
    }

    fn on_graph_start(&mut self) -> Result<()> {
        self.run_initialized = false;
        Ok(())
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![
        port! { name = "session_registry_ref", ty = CurrentSessionRegistryRef, desc = "当前运行期 sender 会话锁注册表引用" },
        port! { name = "current_sender_ids", ty = Vec(String), desc = "当前活跃 sender_id 快照，便于调试和预览" },
    ];

    fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.initialize_run();

        let current_sender_ids = self
            .session_registry_ref
            .current_sender_ids()
            .into_iter()
            .map(DataValue::String)
            .collect();

        let mut outputs = HashMap::new();
        outputs.insert(
            "session_registry_ref".to_string(),
            DataValue::CurrentSessionRegistryRef(self.session_registry_ref.clone()),
        );
        outputs.insert(
            "current_sender_ids".to_string(),
            DataValue::Vec(Box::new(DataType::String), current_sender_ids),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::CurrentSessionListProviderNode;
    use crate::error::Result;
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn provider_returns_registry_and_empty_snapshot() -> Result<()> {
        let mut node = CurrentSessionListProviderNode::new("provider", "Provider");
        let outputs = node.execute(HashMap::new())?;

        assert!(matches!(
            outputs.get("session_registry_ref"),
            Some(DataValue::CurrentSessionRegistryRef(_))
        ));
        assert!(matches!(
            outputs.get("current_sender_ids"),
            Some(DataValue::Vec(_, items)) if items.is_empty()
        ));
        Ok(())
    }
}
