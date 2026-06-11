use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};
use serde_json::Value;
use zihuan_core::error::{Error, Result};

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

    node_input![
        port! { name = "message", ty = LLMMessage, desc = "输入的 LLMMessage，其 content 必须是合法 JSON 字符串" },
    ];

    node_output![
        port! { name = "json", ty = Json, desc = "由 LLMMessage.content 解析得到的 JSON" },
        port! { name = "failed", ty = String, desc = "解析失败时输出原始 content 字符串" },
    ];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let message = match inputs.get("message") {
            Some(DataValue::LLMMessage(message)) => message,
            _ => return Err(Error::InvalidNodeInput("message is required".to_string())),
        };

        let content = message
            .content_text_owned()
            .ok_or_else(|| Error::ValidationError("LLMMessage content is None".to_string()))?;

        match serde_json::from_str(&content) {
            Ok(json) => {
                info!("[{}] LLMMessage content parsed as JSON successfully", self.id);
                crate::return_with_node_output![self;
                    "json" => DataValue::Json(json),
                ]
            }
            Err(err) => {
                // Streaming fallback: the LLM may emit multiple JSON values separated by
                // whitespace (e.g. `[[...]]\n\n[[...]]`) instead of one unified array.
                // Attempt to collect all top-level values; if every one is a JSON array,
                // flat-map their elements into a single array so downstream parsing still works.
                let streamed: Vec<Value> = serde_json::Deserializer::from_str(&content)
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
                        "[{}] LLMMessage content contained multiple JSON values; merged into one array. Original parse error: {}. Raw content: {:?}",
                        self.id, err, content
                    );
                    return crate::return_with_node_output![self;
                        "json" => DataValue::Json(json),
                    ];
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
                        "[{}] LLMMessage content was truncated (missing closing bracket(s)); auto-closed to recover. Original parse error: {}. Raw content: {:?}",
                        self.id, err, content
                    );
                    return crate::return_with_node_output![self;
                        "json" => DataValue::Json(json),
                    ];
                }

                warn!(
                    "[{}] Failed to parse LLMMessage content as JSON: {}. Raw content: {:?}",
                    self.id, err, content
                );
                crate::return_with_node_output![self;
                    "failed" => DataValue::String(content),
                ]
            }
        }
    }
}
