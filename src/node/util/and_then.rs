use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Waits for two inputs, then forwards the second one unchanged.
pub struct AndThenNode {
    id: String,
    name: String,
}

impl AndThenNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AndThenNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("等待 first 和 second 都到齐后，原样透传 second")
    }

    node_input![
        port! { name = "first", ty = Any, desc = "用于串联依赖的前置输入，仅用于等待其到齐" },
        port! { name = "second", ty = Any, desc = "在两个输入都到齐后原样透传的值" },
    ];

    node_output![port! { name = "output", ty = Any, desc = "second 的原样输出" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let output = inputs
            .get("second")
            .cloned()
            .ok_or_else(|| crate::error::Error::ValidationError("second 输入不存在".to_string()))?;

        let outputs = HashMap::from([("output".to_string(), output)]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::AndThenNode;
    use crate::error::Result;
    use zihuan_llm::OpenAIMessage;
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn forwards_second_input_as_output() -> Result<()> {
        let mut node = AndThenNode::new("and_then", "And Then");
        let outputs = node.execute(HashMap::from([
            ("first".to_string(), DataValue::Boolean(true)),
            ("second".to_string(), DataValue::String("hello".to_string())),
        ]))?;

        assert!(matches!(
            outputs.get("output"),
            Some(DataValue::String(value)) if value == "hello"
        ));
        Ok(())
    }

    #[test]
    fn preserves_second_input_shape() -> Result<()> {
        let mut node = AndThenNode::new("and_then", "And Then");
        let outputs = node.execute(HashMap::from([
            ("first".to_string(), DataValue::Integer(1)),
            (
                "second".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    vec![DataValue::OpenAIMessage(OpenAIMessage::user("next"))],
                ),
            ),
        ]))?;

        assert!(matches!(
            outputs.get("output"),
            Some(DataValue::Vec(inner, items))
                if **inner == DataType::OpenAIMessage
                && items.len() == 1
                && matches!(&items[0], DataValue::OpenAIMessage(msg) if msg.content.as_deref() == Some("next"))
        ));
        Ok(())
    }
}
