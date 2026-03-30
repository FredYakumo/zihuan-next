use std::collections::HashMap;

use crate::error::Result;
use crate::llm::{InferenceParam, OpenAIMessage};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};

/// LLMInferNode — Perform a single round of LLM inference.
///
/// Accepts an `LLModel` reference (from `LLMApiNode`) and a message list,
/// calls the model's `inference()` method, and outputs the response as a
/// `Vec<OpenAIMessage>`.
pub struct LLMInferNode {
    id: String,
    name: String,
}

impl LLMInferNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LLMInferNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用LLModel引用对消息列表进行一次推理，返回模型回复")
    }

    node_input![
        port! { name = "llm_model", ty = LLModel,                  desc = "LLM模型引用，由LLMApiNode提供" },
        port! { name = "messages",  ty = Vec(OpenAIMessage), desc = "输入消息列表，包含系统消息和用户消息" },
    ];

    node_output![port! { name = "response", ty = Vec(OpenAIMessage), desc = "LLM返回的消息列表" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let model = match inputs.get("llm_model") {
            Some(DataValue::LLModel(m)) => m.clone(),
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "Missing required input: llm_model".to_string(),
                ))
            }
        };

        let messages: Vec<OpenAIMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|item| {
                    if let DataValue::OpenAIMessage(m) = item {
                        Some(m.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "Missing required input: messages".to_string(),
                ))
            }
        };

        let param = InferenceParam {
            messages: &messages,
            tools: None,
        };

        let response_message = model.inference(&param);

        let mut outputs = HashMap::new();
        outputs.insert(
            "response".to_string(),
            DataValue::Vec(
                Box::new(DataType::OpenAIMessage),
                vec![DataValue::OpenAIMessage(response_message)],
            ),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
