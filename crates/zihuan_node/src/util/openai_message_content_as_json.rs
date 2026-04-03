use zihuan_core::error::{Error, Result};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};
use std::collections::HashMap;

/// Parses the `content` string of an `OpenAIMessage` into JSON.
pub struct OpenAIMessageContentAsJsonNode {
    id: String,
    name: String,
}

impl OpenAIMessageContentAsJsonNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for OpenAIMessageContentAsJsonNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 OpenAIMessage 的 content 字符串解析为 JSON")
    }

    node_input![port! { name = "message", ty = OpenAIMessage, desc = "输入的 OpenAIMessage，其 content 必须是合法 JSON 字符串" },];

    node_output![
        port! { name = "json", ty = Json, desc = "由 OpenAIMessage.content 解析得到的 JSON" },
        port! { name = "failed", ty = String, desc = "解析失败时输出原始 content 字符串" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let message = match inputs.get("message") {
            Some(DataValue::OpenAIMessage(message)) => message,
            _ => return Err(Error::InvalidNodeInput("message is required".to_string())),
        };

        let content = message.content.as_ref().ok_or_else(|| {
            Error::ValidationError("OpenAIMessage content is None".to_string())
        })?;

        let outputs = match serde_json::from_str(content) {
            Ok(json) => {
                info!(
                    "[{}] OpenAIMessage content parsed as JSON successfully",
                    self.id
                );
                HashMap::from([("json".to_string(), DataValue::Json(json))])
            }
            Err(err) => {
                warn!(
                    "[{}] Failed to parse OpenAIMessage content as JSON: {}. Raw content: {:?}",
                    self.id, err, content
                );
                HashMap::from([("failed".to_string(), DataValue::String(content.clone()))])
            }
        };
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAIMessageContentAsJsonNode;
    use zihuan_llm_types::{MessageRole, OpenAIMessage};
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    fn message(content: Option<&str>) -> OpenAIMessage {
        OpenAIMessage {
            role: MessageRole::Assistant,
            content: content.map(ToString::to_string),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    #[test]
    fn parses_message_content_to_json() {
        let mut node = OpenAIMessageContentAsJsonNode::new("json_1", "MessageContentAsJson");
        let outputs = node
            .execute(HashMap::from([(
                "message".to_string(),
                DataValue::OpenAIMessage(message(Some(r#"{"reply":"你好","ok":true}"#))),
            )]))
            .expect("message content json should parse");

        match outputs.get("json") {
            Some(DataValue::Json(value)) => {
                assert_eq!(value["reply"], "你好");
                assert_eq!(value["ok"], true);
            }
            other => panic!("unexpected json output: {:?}", other),
        }
    }

    #[test]
    fn rejects_missing_content() {
        let mut node = OpenAIMessageContentAsJsonNode::new("json_2", "MessageContentAsJson");
        let err = node
            .execute(HashMap::from([(
                "message".to_string(),
                DataValue::OpenAIMessage(message(None)),
            )]))
            .expect_err("missing content should fail");

        assert!(err.to_string().contains("content"));
    }

    #[test]
    fn routes_invalid_json_to_failed_output() {
        let mut node = OpenAIMessageContentAsJsonNode::new("json_3", "MessageContentAsJson");
        let outputs = node
            .execute(HashMap::from([(
                "message".to_string(),
                DataValue::OpenAIMessage(message(Some("not-json"))),
            )]))
            .expect("invalid json should not return Err, just route to failed");

        match outputs.get("failed") {
            Some(DataValue::String(raw)) => assert_eq!(raw, "not-json"),
            other => panic!("expected failed output with raw string, got: {:?}", other),
        }
        assert!(!outputs.contains_key("json"), "json port should not be set on failure");
    }
}
