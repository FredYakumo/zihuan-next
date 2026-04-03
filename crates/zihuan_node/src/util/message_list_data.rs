use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct MessageListDataNode {
    id: String,
    name: String,
}

impl MessageListDataNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageListDataNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("OpenAIMessage 列表数据源，支持内联 UI 编辑")
    }

    node_input![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Vec<OpenAIMessage> provided by UI inline editor", optional },
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Output Vec<OpenAIMessage> from UI data source" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        let value = inputs
            .get("messages")
            .cloned()
            .filter(|v| matches!(v, DataValue::Vec(..)))
            .unwrap_or_else(|| {
                DataValue::Vec(Box::new(crate::DataType::OpenAIMessage), Vec::new())
            });
        outputs.insert("messages".to_string(), value);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
