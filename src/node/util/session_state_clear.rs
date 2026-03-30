use crate::error::Result;
use crate::node::data_value::SessionStateRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

pub struct SessionStateClearNode {
    id: String,
    name: String,
}

impl SessionStateClearNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SessionStateClearNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("清除 sender_id 的会话状态")
    }

    node_input![
        port! { name = "session_ref", ty = SessionStateRef, desc = "运行时会话状态引用" },
        port! { name = "sender_id", ty = String, desc = "会话发送者 ID" },
    ];

    node_output![
        port! { name = "cleared", ty = Boolean, desc = "是否清除了状态" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let session_ref: Arc<SessionStateRef> = inputs
            .get("session_ref")
            .and_then(|value| match value {
                DataValue::SessionStateRef(session_ref) => Some(session_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("session_ref is required".to_string()))?;
        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let clear_state = async move { session_ref.clear_state(&sender_id).await };
        let cleared = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(clear_state))
        } else {
            tokio::runtime::Runtime::new()?.block_on(clear_state)
        };

        let outputs = HashMap::from([("cleared".to_string(), DataValue::Boolean(cleared))]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
