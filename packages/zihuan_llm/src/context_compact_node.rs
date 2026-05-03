use std::collections::HashMap;

use zihuan_core::error::{Error, Result};
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

use crate::context_compaction::compact_context_messages;
use crate::OpenAIMessage;

pub struct ContextCompactNode {
    id: String,
    name: String,
}

impl ContextCompactNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn parse_messages_input(
        &self,
        inputs: &HashMap<String, DataValue>,
    ) -> Result<Vec<OpenAIMessage>> {
        match inputs.get("messages") {
            Some(DataValue::Vec(inner_type, items)) if **inner_type == DataType::OpenAIMessage => {
                items
                    .iter()
                    .map(|item| match item {
                        DataValue::OpenAIMessage(message) => Ok(message.clone()),
                        _ => Err(self.wrap_error("messages must contain OpenAIMessage items")),
                    })
                    .collect()
            }
            _ => Err(self.wrap_error("messages is required")),
        }
    }

    fn wrap_error(&self, message: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, message.into()))
    }
}

impl Node for ContextCompactNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("压缩 OpenAIMessage 历史为摘要对加最近 2 条非 tool 消息")
    }

    node_input![
        port! { name = "llm_model", ty = LLModel, desc = "LLM 模型引用，用于执行上下文摘要压缩" },
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "待压缩的 OpenAIMessage 历史列表" },
        port! { name = "compact_context_length", ty = Integer, desc = "估算 token 超过该阈值时触发压缩；<=0 时仅 force_compact 可触发" },
        port! { name = "force_compact", ty = Boolean, desc = "为 true 时即使未超阈值也执行压缩", optional },
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "压缩后的消息列表" },
        port! { name = "did_compact", ty = Boolean, desc = "本次是否执行并应用了压缩" },
        port! { name = "estimated_tokens_before", ty = Integer, desc = "压缩前的估算 token 数" },
        port! { name = "estimated_tokens_after", ty = Integer, desc = "压缩后的估算 token 数" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let llm = match inputs.get("llm_model") {
            Some(DataValue::LLModel(model)) => model.clone(),
            _ => return Err(self.wrap_error("llm_model is required")),
        };
        let messages = self.parse_messages_input(&inputs)?;
        let compact_context_length = match inputs.get("compact_context_length") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as usize,
            Some(DataValue::Integer(_)) | None => 0,
            _ => return Err(self.wrap_error("compact_context_length must be an integer")),
        };
        let force_compact = matches!(inputs.get("force_compact"), Some(DataValue::Boolean(true)));

        let result =
            compact_context_messages(&llm, messages, compact_context_length, &[], force_compact);

        let mut outputs = HashMap::new();
        outputs.insert(
            "messages".to_string(),
            DataValue::Vec(
                Box::new(DataType::OpenAIMessage),
                result
                    .messages
                    .into_iter()
                    .map(DataValue::OpenAIMessage)
                    .collect(),
            ),
        );
        outputs.insert(
            "did_compact".to_string(),
            DataValue::Boolean(result.did_compact),
        );
        outputs.insert(
            "estimated_tokens_before".to_string(),
            DataValue::Integer(result.estimated_tokens_before as i64),
        );
        outputs.insert(
            "estimated_tokens_after".to_string(),
            DataValue::Integer(result.estimated_tokens_after as i64),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
