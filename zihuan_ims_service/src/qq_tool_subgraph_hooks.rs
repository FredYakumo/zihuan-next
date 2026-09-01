use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::agent::tools::{
    consume_tool_progress_notification, current_task_progress_message,
};
use zihuan_core::graph::tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
};
use zihuan_core::graph::DataValue;
use zihuan_core::ims_bot_adapter::adapter::shared_from_handle;
use zihuan_core::ims_bot_adapter::models::MessageType;
use zihuan_core::task_context::append_current_task_progress;
use zihuan_core::tool_subgraph::{BuiltinToolExecutor, ToolProgressNotifier};

use crate::qq_chat::msg_send::{send_notification_text, QqChatServiceSendContext};
use crate::tools::{execute_image_understand_tool, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS};

pub fn image_understand_executor() -> BuiltinToolExecutor {
    Arc::new(|arguments, runtime_values| execute_image_understand_tool(arguments, runtime_values))
}

pub fn qq_progress_notifier() -> ToolProgressNotifier {
    Arc::new(send_brain_tool_progress_notification)
}

fn send_brain_tool_progress_notification(
    runtime_values: &HashMap<String, DataValue>,
    call_content: &str,
) {
    if let Some(progress_text) = current_task_progress_message(call_content) {
        if append_current_task_progress(progress_text) {
            return;
        }
    }

    if matches!(
        runtime_values.get(QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS),
        Some(DataValue::Boolean(false))
    ) {
        return;
    }

    if !consume_tool_progress_notification(call_content) {
        return;
    }

    let event = match runtime_values.get(QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT) {
        Some(DataValue::MessageEvent(event)) => event,
        _ => return,
    };
    let adapter = match runtime_values.get(QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT) {
        Some(DataValue::BotAdapterRef(handle)) => shared_from_handle(handle),
        _ => return,
    };

    if event.message_type == MessageType::Group {
        if let Some(group_id) = event.group_id {
            let group_id = group_id.to_string();
            let sender_id = event.sender.user_id.to_string();
            let send_context = QqChatServiceSendContext {
                adapter: &adapter,
                target_id: &group_id,
                is_group: true,
                group_name: event.group_name.as_deref(),
                bot_id: "",
                bot_name: "",
                mention_target_id: Some(&sender_id),
                persistence: Default::default(),
                max_text_chars: 250,
            };
            let _ = send_notification_text(&send_context, call_content);
        }
        return;
    }

    let target_id = event.sender.user_id.to_string();
    let send_context = QqChatServiceSendContext {
        adapter: &adapter,
        target_id: &target_id,
        is_group: false,
        group_name: None,
        bot_id: "",
        bot_name: "",
        mention_target_id: None,
        persistence: Default::default(),
        max_text_chars: 250,
    };
    let _ = send_notification_text(&send_context, call_content);
}
