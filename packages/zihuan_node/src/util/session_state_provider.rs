use crate::data_value::SessionStateRef;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use zihuan_core::error::Result;

pub struct SessionStateProviderNode {
    id: String,
    name: String,
    session_ref: Arc<SessionStateRef>,
}

impl SessionStateProviderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            name: name.into(),
            session_ref: Arc::new(SessionStateRef::new(id)),
        }
    }
}

impl Node for SessionStateProviderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("创建跨事件共享的运行时会话状态引用")
    }

    node_input![];

    node_output![port! { name = "session_ref", ty = SessionStateRef, desc = "运行时会话状态引用" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let outputs = HashMap::from([(
            "session_ref".to_string(),
            DataValue::SessionStateRef(self.session_ref.clone()),
        )]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
