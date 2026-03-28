use crate::error::Result;
use crate::llm::MessageRole;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Checks whether the last message in a Vec<OpenAIMessage> has role=tool.
pub struct LastMessageIsToolNode {
    id: String,
    name: String,
}

impl LastMessageIsToolNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LastMessageIsToolNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("判断 Vec<OpenAIMessage> 最后一条消息是否为 role=tool")
    }

    node_input![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "OpenAIMessage 列表" },
    ];

    node_output![
        port! { name = "result", ty = Boolean, desc = "最后一条消息是否为 role=tool" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let result = match inputs.get("messages") {
            Some(DataValue::Vec(_, items)) => {
                items.last().is_some_and(|last| {
                    matches!(last, DataValue::OpenAIMessage(m) if m.role == MessageRole::Tool)
                })
            }
            _ => false,
        };

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::Boolean(result));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::LastMessageIsToolNode;
    use crate::llm::{MessageRole, OpenAIMessage};
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    fn msg(role: MessageRole) -> DataValue {
        DataValue::OpenAIMessage(OpenAIMessage {
            role,
            content: Some("test".to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        })
    }

    fn run(messages: Vec<DataValue>) -> bool {
        let mut node = LastMessageIsToolNode::new("n", "N");
        let inputs = HashMap::from([(
            "messages".to_string(),
            DataValue::Vec(Box::new(DataType::OpenAIMessage), messages),
        )]);
        let outputs = node.execute(inputs).unwrap();
        match outputs.get("result") {
            Some(DataValue::Boolean(b)) => *b,
            _ => panic!("unexpected output"),
        }
    }

    #[test]
    fn last_tool_returns_true() {
        assert!(run(vec![
            msg(MessageRole::User),
            msg(MessageRole::Assistant),
            msg(MessageRole::Tool),
        ]));
    }

    #[test]
    fn last_assistant_returns_false() {
        assert!(!run(vec![
            msg(MessageRole::User),
            msg(MessageRole::Assistant),
        ]));
    }

    #[test]
    fn empty_list_returns_false() {
        assert!(!run(vec![]));
    }
}
