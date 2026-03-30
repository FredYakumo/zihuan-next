use crate::error::Result;
use crate::node::data_value::CurrentSessionRegistryRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SenderIdInCurrentSessionNode {
    id: String,
    name: String,
}

impl SenderIdInCurrentSessionNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SenderIdInCurrentSessionNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("判断 sender_id 当前是否已经在会话列表中")
    }

    node_input![
        port! { name = "session_registry_ref", ty = CurrentSessionRegistryRef, desc = "当前运行期 sender 会话锁注册表引用" },
        port! { name = "sender_id", ty = String, desc = "要检查的 sender_id" },
    ];

    node_output![
        port! { name = "in_session", ty = Boolean, desc = "sender_id 当前是否已在会话列表中" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let registry_ref: Arc<CurrentSessionRegistryRef> = inputs
            .get("session_registry_ref")
            .and_then(|value| match value {
                DataValue::CurrentSessionRegistryRef(registry_ref) => Some(registry_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("session_registry_ref is required".to_string()))?;

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let mut outputs = HashMap::new();
        outputs.insert(
            "in_session".to_string(),
            DataValue::Boolean(registry_ref.contains_sender_id(&sender_id)),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
