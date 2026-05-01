use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_bot_types::natural_language_reply::qq_message_json_output_system_prompt;
use zihuan_core::error::Result;

pub struct QQMessageJsonOutputSystemPromptProviderNode {
    id: String,
    name: String,
}

impl QQMessageJsonOutputSystemPromptProviderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for QQMessageJsonOutputSystemPromptProviderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("输出固定的 system prompt，要求 LLM 只返回 QQ 消息二维 JSON 数组")
    }

    node_input![];

    node_output![
        port! { name = "system_prompt", ty = String, desc = "固定的 QQ 消息二维 JSON 输出格式 prompt" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let outputs = HashMap::from([(
            "system_prompt".to_string(),
            DataValue::String(qq_message_json_output_system_prompt().to_string()),
        )]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::QQMessageJsonOutputSystemPromptProviderNode;
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn outputs_fixed_system_prompt() {
        let mut node =
            QQMessageJsonOutputSystemPromptProviderNode::new("provider", "Prompt Provider");
        let outputs = node
            .execute(HashMap::new())
            .expect("provider node should execute");

        match outputs.get("system_prompt") {
            Some(DataValue::String(value)) => {
                assert!(!value.is_empty());
                assert!(value.contains("纯 JSON 数组"));
            }
            other => panic!("unexpected system_prompt output: {:?}", other),
        }
    }
}
