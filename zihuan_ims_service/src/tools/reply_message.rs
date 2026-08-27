use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use zihuan_core::agent::tools::Tool;
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::graph::DataValue;

use crate::qq_chat::msg_send::{store_reply_directive, QqChatServiceReplyDirective};

use super::common::StaticFunctionToolSpec;

pub(crate) struct ReplyMessageTool {
    shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
}

impl ReplyMessageTool {
    pub(crate) fn new(shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>) -> Self {
        Self { shared_runtime_values }
    }
}

impl Tool for ReplyMessageTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "reply_message",
            description: "设置本轮最终回复要引用的 QQ 消息；可选指定 message_id，不传则默认引用当前触发消息。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "integer",
                        "description": "可选：要引用的 QQ message_id；不传时默认引用当前触发消息"
                    }
                },
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let directive = match arguments.get("message_id") {
                Some(Value::Number(number)) => {
                    let message_id = number
                        .as_i64()
                        .ok_or_else(|| Error::ValidationError("message_id must be an integer".to_string()))?;
                    if message_id <= 0 {
                        return Err(Error::ValidationError("message_id must be a positive integer".to_string()));
                    }
                    QqChatServiceReplyDirective::Explicit { message_id }
                }
                Some(Value::Null) | None => QqChatServiceReplyDirective::TriggerMessage,
                Some(_) => {
                    return Err(Error::ValidationError(
                        "message_id must be an integer when provided".to_string(),
                    ))
                }
            };

            store_reply_directive(&self.shared_runtime_values, directive);
            Ok("已记录 reply_message 设置。".to_string())
        })();

        match result {
            Ok(message) => message,
            Err(error) => serde_json::json!({
                "ok": false,
                "error": error.to_string(),
            })
            .to_string(),
        }
    }
}
