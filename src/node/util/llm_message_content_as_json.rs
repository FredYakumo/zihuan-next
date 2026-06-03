use crate::error::{Error, Result};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};
use std::collections::HashMap;

/// Parses the `content` string of an `LLMMessage` into JSON.
pub struct LLMMessageContentAsJsonNode {
    id: String,
    name: String,
}

impl LLMMessageContentAsJsonNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LLMMessageContentAsJsonNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 LLMMessage 的 content 字符串解析为 JSON")
    }

    node_input![port! { name = "message", ty = LLMMessage, desc = "输入的 LLMMessage，其 content 必须是合法 JSON 字符串" },];

    node_output![
        port! { name = "json", ty = Json, desc = "由 LLMMessage.content 解析得到的 JSON" },
        port! { name = "failed", ty = String, desc = "解析失败时输出原始 content 字符串" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let message = match inputs.get("message") {
            Some(DataValue::LLMMessage(message)) => message,
            _ => return Err(Error::InvalidNodeInput("message is required".to_string())),
        };

        let content = message.content.as_ref().ok_or_else(|| {
            Error::ValidationError("LLMMessage content is None".to_string())
        })?;

        let outputs = match serde_json::from_str(content) {
            Ok(json) => {
                info!(
                    "[{}] LLMMessage content parsed as JSON successfully",
                    self.id
                );
                HashMap::from([("json".to_string(), DataValue::Json(json))])
            }
            Err(err) => {
                warn!(
                    "[{}] Failed to parse LLMMessage content as JSON: {}. Raw content: {:?}",
                    self.id, err, content
                );
                HashMap::from([("failed".to_string(), DataValue::String(content.clone()))])
            }
        };
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

