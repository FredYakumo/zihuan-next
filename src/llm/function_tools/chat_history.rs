use super::FunctionTool;
use crate::error::Result;
use serde_json::{json, Value};
use std::env;

/// Fetch chat/event record by message_id from Redis (if available).
///
/// Notes:
/// - Requires REDIS_URL to be set. Falls back to an informational error when not set.
/// - The adapter stores RawMessageEvent JSON under key = message_id.
#[derive(Debug, Default)]
pub struct ChatHistoryTool;

impl ChatHistoryTool {
    pub fn new() -> Self { Self }
}

impl FunctionTool for ChatHistoryTool {
    fn name(&self) -> &str { "chat_history" }

    fn description(&self) -> &str {
        "Fetch a stored chat/event by message_id from Redis. Returns the raw event JSON if found."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "string", "description": "The event message_id to fetch" }
            },
            "required": ["message_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> Result<Value> {
        let message_id = arguments
            .get("message_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::string_error!("missing required parameter: message_id"))?;

        let redis_url = env::var("REDIS_URL").ok();
        if redis_url.is_none() {
            return Err(crate::string_error!("REDIS_URL not set; chat history unavailable without Redis"));
        }
        let redis_url = redis_url.unwrap();

        // Use blocking Redis client to fetch stored JSON string
        let client = redis::Client::open(redis_url)
            .map_err(|e| crate::string_error!("invalid REDIS_URL: {}", e))?;
        let mut conn = client
            .get_connection()
            .map_err(|e| crate::string_error!("failed to connect to Redis: {}", e))?;

        let val: Option<String> = redis::Commands::get(&mut conn, message_id)
            .map_err(|e| crate::string_error!("Redis GET failed: {}", e))?;

        match val {
            Some(s) => {
                // Try to parse as JSON, otherwise return as string
                let parsed = serde_json::from_str::<Value>(&s).unwrap_or_else(|_| json!({"raw": s }));
                Ok(json!({
                    "message_id": message_id,
                    "event": parsed
                }))
            }
            None => Err(crate::string_error!("no record found for message_id={}", message_id)),
        }
    }
}

