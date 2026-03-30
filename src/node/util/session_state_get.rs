use crate::error::Result;
use crate::node::data_value::SessionStateRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

pub struct SessionStateGetNode {
    id: String,
    name: String,
}

impl SessionStateGetNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SessionStateGetNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("读取 sender_id 的会话状态")
    }

    node_input![
        port! { name = "session_ref", ty = SessionStateRef, desc = "运行时会话状态引用" },
        port! { name = "sender_id", ty = String, desc = "会话发送者 ID" },
    ];

    node_output![
        port! { name = "in_session", ty = Boolean, desc = "当前 sender_id 是否正在处理中" },
        port! { name = "state_json", ty = Json, desc = "当前 sender_id 的附加 JSON 状态" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let session_ref: Arc<SessionStateRef> = inputs
            .get("session_ref")
            .and_then(|value| match value {
                DataValue::SessionStateRef(session_ref) => Some(session_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("session_ref is required".to_string())
            })?;
        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("sender_id is required".to_string())
            })?;

        let read_state = async move { session_ref.get_state(&sender_id).await };
        let state = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(read_state))
        } else {
            tokio::runtime::Runtime::new()?.block_on(read_state)
        };

        let outputs = HashMap::from([
            (
                "in_session".to_string(),
                DataValue::Boolean(state.in_session),
            ),
            ("state_json".to_string(), DataValue::Json(state.state_json)),
        ]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
