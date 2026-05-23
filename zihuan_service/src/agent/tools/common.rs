use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::warn;
use serde_json::Value;

use ims_bot_adapter::adapter::{shared_from_handle, SharedBotAdapter};
use ims_bot_adapter::message_helpers::{
    send_friend_progress_notification_with_persistence,
    send_group_progress_notification_with_persistence, OutboundMessagePersistence,
};
use ims_bot_adapter::models::event_model::MessageType;
use zihuan_agent::brain::consume_tool_progress_notification;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{DataType, DataValue};

const LOG_PREFIX: &str = "[QqChatAgent]";

#[derive(Clone)]
pub(crate) struct ToolNotificationTarget {
    adapter: Option<SharedBotAdapter>,
    target_id: String,
    mention_target_id: Option<String>,
    is_group: bool,
}

impl ToolNotificationTarget {
    pub(crate) fn new(
        adapter: Option<SharedBotAdapter>,
        target_id: String,
        mention_target_id: Option<String>,
        is_group: bool,
    ) -> Self {
        Self {
            adapter,
            target_id,
            mention_target_id,
            is_group,
        }
    }

    pub(crate) fn dashboard() -> Self {
        Self::new(None, String::new(), None, false)
    }

    pub(crate) fn notify_progress(&self, call_content: &str) {
        // Skip empty or already-consumed progress content to avoid duplicate notifications.
        if !consume_tool_progress_notification(call_content) {
            return;
        }
        let Some(adapter) = self.adapter.as_ref() else {
            return;
        };
        if self.is_group {
            if let Some(mid) = self.mention_target_id.as_deref() {
                send_group_progress_notification_with_persistence(
                    adapter,
                    &self.target_id,
                    mid,
                    call_content,
                    &OutboundMessagePersistence::default(),
                );
            }
        } else {
            send_friend_progress_notification_with_persistence(
                adapter,
                &self.target_id,
                call_content,
                &OutboundMessagePersistence::default(),
            );
        }
    }

    pub(crate) fn target_id(&self) -> &str {
        &self.target_id
    }

    pub(crate) fn is_group(&self) -> bool {
        self.is_group
    }
}

/// Sends a progress notification by extracting the bot adapter and event target
/// from the shared graph runtime values. Used when the tool does not have a
/// pre-built `ToolNotificationTarget` and must resolve the destination dynamically.
pub(crate) fn send_editable_tool_progress_notification(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
    call_content: &str,
) {
    // Deduplicate and skip empty progress; the same content may stream in repeatedly.
    if !consume_tool_progress_notification(call_content) {
        return;
    }

    let shared_rt = shared_runtime_values.lock().unwrap();
    let event = match shared_rt
        .get(zihuan_graph_engine::brain_tool_spec::QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT)
    {
        Some(DataValue::MessageEvent(event)) => event,
        _ => {
            warn!(
                "{LOG_PREFIX} editable tool progress notification skipped: missing message_event"
            );
            return;
        }
    };
    let adapter = match shared_rt
        .get(zihuan_graph_engine::brain_tool_spec::QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT)
    {
        Some(DataValue::BotAdapterRef(handle)) => shared_from_handle(handle),
        _ => {
            warn!(
                "{LOG_PREFIX} editable tool progress notification skipped: missing qq_ims_bot_adapter"
            );
            return;
        }
    };

    if event.message_type == MessageType::Group {
        if let Some(group_id) = event.group_id {
            let sender_id = event.sender.user_id.to_string();
            send_group_progress_notification_with_persistence(
                &adapter,
                &group_id.to_string(),
                &sender_id,
                call_content,
                &OutboundMessagePersistence {
                    group_name: event.group_name.clone(),
                    ..OutboundMessagePersistence::default()
                },
            );
        } else {
            warn!(
                "{LOG_PREFIX} editable tool progress notification skipped: group message missing group_id"
            );
        }
    } else {
        send_friend_progress_notification_with_persistence(
            &adapter,
            &event.sender.user_id.to_string(),
            call_content,
            &OutboundMessagePersistence::default(),
        );
    }
}

/// Coerces an optional limit into a bounded positive usize, falling back to
/// `default_limit` and clamping between 1 and `max_limit`.
pub(crate) fn sanitize_positive_limit(
    value: Option<i64>,
    default_limit: i64,
    max_limit: i64,
) -> usize {
    let limit = value.unwrap_or(default_limit);
    limit.clamp(1, max_limit) as usize
}

/// Extracts a trimmed, non-empty string from a JSON object field if present.
pub(crate) fn optional_string_argument(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn optional_bool_argument(arguments: &Value, key: &str) -> Option<bool> {
    arguments.get(key).and_then(Value::as_bool)
}

pub(crate) fn extract_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Extracts a `Vec<String>` from a graph output map, validating both the outer
/// vector type and that every inner item is actually a string.
pub(crate) fn extract_string_list_output(
    outputs: &HashMap<String, DataValue>,
    key: &str,
) -> Result<Vec<String>> {
    let value = outputs
        .get(key)
        .ok_or_else(|| Error::ValidationError(format!("missing output: {key}")))?;
    match value {
        DataValue::Vec(inner, items) if **inner == DataType::String => {
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    DataValue::String(value) => result.push(value.clone()),
                    other => {
                        return Err(Error::ValidationError(format!(
                            "expected String item in {key}, got {}",
                            other.data_type()
                        )))
                    }
                }
            }
            Ok(result)
        }
        other => Err(Error::ValidationError(format!(
            "{key} must be Vec<String>, got {}",
            other.data_type()
        ))),
    }
}

#[derive(Debug)]
pub(crate) struct StaticFunctionToolSpec {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) parameters: Value,
}

impl zihuan_core::llm::tooling::FunctionTool for StaticFunctionToolSpec {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    fn call(&self, _arguments: Value) -> Result<Value> {
        Ok(Value::Null)
    }
}
