use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Routes one of two inputs to the output based on a boolean condition.
pub struct ConditionalRouterNode {
    id: String,
    name: String,
}

impl ConditionalRouterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ConditionalRouterNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("按布尔条件在 primary 和 fallback 两路输入之间选择一路输出")
    }

    node_input![
        port! { name = "condition", ty = Boolean, desc = "条件为 true 时选择 primary，否则选择 fallback" },
        port! { name = "primary", ty = Any, desc = "condition=true 时输出的值" },
        port! { name = "fallback", ty = Any, desc = "condition=false 时输出的值" },
    ];

    node_output![
        port! { name = "result", ty = Any, desc = "被选中的输入值，原样透传" },
        port! { name = "branch_taken", ty = String, desc = "实际走到的分支：primary 或 fallback" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let condition = match inputs.get("condition") {
            Some(DataValue::Boolean(value)) => *value,
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "condition 输入必须为 Boolean".to_string(),
                ))
            }
        };

        let (result, branch_taken) = if condition {
            (
                inputs.get("primary").cloned().ok_or_else(|| {
                    crate::error::Error::ValidationError("primary 输入不存在".to_string())
                })?,
                "primary",
            )
        } else {
            (
                inputs.get("fallback").cloned().ok_or_else(|| {
                    crate::error::Error::ValidationError("fallback 输入不存在".to_string())
                })?,
                "fallback",
            )
        };

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), result);
        outputs.insert(
            "branch_taken".to_string(),
            DataValue::String(branch_taken.to_string()),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::ConditionalRouterNode;
    use crate::error::Result;
    use crate::llm::{MessageRole, OpenAIMessage};
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    fn assistant_message(content: &str) -> OpenAIMessage {
        OpenAIMessage {
            role: MessageRole::Assistant,
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    #[test]
    fn returns_primary_when_condition_is_true() -> Result<()> {
        let mut node = ConditionalRouterNode::new("router", "Router");
        let outputs = node.execute(HashMap::from([
            ("condition".to_string(), DataValue::Boolean(true)),
            ("primary".to_string(), DataValue::String("new".to_string())),
            ("fallback".to_string(), DataValue::String("old".to_string())),
        ]))?;

        assert!(matches!(
            outputs.get("result"),
            Some(DataValue::String(value)) if value == "new"
        ));
        assert!(matches!(
            outputs.get("branch_taken"),
            Some(DataValue::String(value)) if value == "primary"
        ));
        Ok(())
    }

    #[test]
    fn returns_fallback_when_condition_is_false() -> Result<()> {
        let mut node = ConditionalRouterNode::new("router", "Router");
        let outputs = node.execute(HashMap::from([
            ("condition".to_string(), DataValue::Boolean(false)),
            ("primary".to_string(), DataValue::Integer(42)),
            ("fallback".to_string(), DataValue::Boolean(true)),
        ]))?;

        assert!(matches!(
            outputs.get("result"),
            Some(DataValue::Boolean(true))
        ));
        assert!(matches!(
            outputs.get("branch_taken"),
            Some(DataValue::String(value)) if value == "fallback"
        ));
        Ok(())
    }

    #[test]
    fn preserves_message_list_shape_for_brain_messages() -> Result<()> {
        let mut node = ConditionalRouterNode::new("router", "Router");
        let fallback = DataValue::Vec(
            Box::new(DataType::OpenAIMessage),
            vec![DataValue::OpenAIMessage(OpenAIMessage::user("first"))],
        );
        let primary = DataValue::Vec(
            Box::new(DataType::OpenAIMessage),
            vec![DataValue::OpenAIMessage(assistant_message("next"))],
        );

        let first_outputs = node.execute(HashMap::from([
            ("condition".to_string(), DataValue::Boolean(false)),
            ("primary".to_string(), primary.clone()),
            ("fallback".to_string(), fallback.clone()),
        ]))?;
        assert!(matches!(
            first_outputs.get("result"),
            Some(DataValue::Vec(inner, items))
                if **inner == DataType::OpenAIMessage
                && items.len() == 1
                && matches!(&items[0], DataValue::OpenAIMessage(msg) if msg.content.as_deref() == Some("first"))
        ));

        let next_outputs = node.execute(HashMap::from([
            ("condition".to_string(), DataValue::Boolean(true)),
            ("primary".to_string(), primary),
            ("fallback".to_string(), fallback),
        ]))?;
        assert!(matches!(
            next_outputs.get("result"),
            Some(DataValue::Vec(inner, items))
                if **inner == DataType::OpenAIMessage
                && items.len() == 1
                && matches!(&items[0], DataValue::OpenAIMessage(msg) if msg.content.as_deref() == Some("next"))
        ));

        Ok(())
    }
}
