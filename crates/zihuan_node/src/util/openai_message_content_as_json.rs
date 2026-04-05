use zihuan_core::error::{Error, Result};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};
use serde_json::Value;
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
                // Streaming fallback: the LLM may emit multiple JSON values separated by
                // whitespace (e.g. `[[...]]\n\n[[...]]`) instead of one unified array.
                // Attempt to collect all top-level values; if every one is a JSON array,
                // flat-map their elements into a single array so downstream parsing still works.
                let streamed: Vec<Value> = serde_json::Deserializer::from_str(content)
                    .into_iter::<Value>()
                    .filter_map(|r| r.ok())
                    .collect();

                let merged = if !streamed.is_empty() && streamed.iter().all(|v| v.is_array()) {
                    let elements: Vec<Value> = streamed
                        .into_iter()
                        .flat_map(|v| match v {
                            Value::Array(arr) => arr,
                            _ => vec![],
                        })
                        .collect();
                    Some(Value::Array(elements))
                } else {
                    None
                };

                if let Some(json) = merged {
                    warn!(
                        "[{}] OpenAIMessage content contained multiple JSON values; merged into one array. Original parse error: {}. Raw content: {:?}",
                        self.id, err, content
                    );
                    return {
                        let outputs = HashMap::from([("json".to_string(), DataValue::Json(json))]);
                        self.validate_outputs(&outputs)?;
                        Ok(outputs)
                    };
                }

                // Bracket-closing fallback: the LLM may have truncated the output before
                // closing all brackets (e.g. `[[a],[b],[c]` missing the final `]`).
                // Try appending 1–3 closing brackets to see if the result becomes valid JSON.
                let mut bracket_recovered: Option<Value> = None;
                for suffix in &["]", "]]", "]]]"] {
                    let candidate = format!("{content}{suffix}");
                    if let Ok(v) = serde_json::from_str::<Value>(&candidate) {
                        if v.is_array() {
                            bracket_recovered = Some(v);
                            break;
                        }
                    }
                }

                if let Some(json) = bracket_recovered {
                    warn!(
                        "[{}] OpenAIMessage content was truncated (missing closing bracket(s)); auto-closed to recover. Original parse error: {}. Raw content: {:?}",
                        self.id, err, content
                    );
                    return {
                        let outputs = HashMap::from([("json".to_string(), DataValue::Json(json))]);
                        self.validate_outputs(&outputs)?;
                        Ok(outputs)
                    };
                }

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

    #[test]
    fn merges_multiple_json_array_blocks() {
        let mut node = OpenAIMessageContentAsJsonNode::new("json_4", "MessageContentAsJson");
        // Simulate LLM emitting several [[...]] blocks separated by newlines
        let content = r#"[["a"],["b"]]

[["c"]]"#;
        let outputs = node
            .execute(HashMap::from([(
                "message".to_string(),
                DataValue::OpenAIMessage(message(Some(content))),
            )]))
            .expect("multi-block content should not return Err");

        match outputs.get("json") {
            Some(DataValue::Json(value)) => {
                let arr = value.as_array().expect("merged result must be an array");
                assert_eq!(arr.len(), 3, "should have 3 inner batches after merging");
            }
            other => panic!("expected json output, got: {:?}", other),
        }
        assert!(!outputs.contains_key("failed"));
    }

    #[test]
    fn recovers_truncated_json_by_closing_brackets() {
        let mut node = OpenAIMessageContentAsJsonNode::new("json_5", "MessageContentAsJson");
        // Simulate LLM truncating output before the final closing bracket
        let content = r#"[[{"message_type":"plain_text","content":"hello"}],[{"message_type":"plain_text","content":"world"}]"#;
        let outputs = node
            .execute(HashMap::from([(
                "message".to_string(),
                DataValue::OpenAIMessage(message(Some(content))),
            )]))
            .expect("truncated content should not return Err");

        match outputs.get("json") {
            Some(DataValue::Json(value)) => {
                let arr = value.as_array().expect("recovered result must be an array");
                assert_eq!(arr.len(), 2, "should have 2 inner batches after bracket-closing");
            }
            other => panic!("expected json output, got: {:?}", other),
        }
        assert!(!outputs.contains_key("failed"));
    }
}
