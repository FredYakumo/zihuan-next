pub mod active_adapter_manager;
pub mod adapter;
pub mod event;
pub mod extract_group_id_from_event;
pub mod extract_message_from_event;
pub mod extract_optional_group_id_from_event;
pub mod extract_qq_message_list_from_event;
pub mod extract_sender_from_event;
pub mod extract_sender_id_from_event;
pub mod ims_bot_adapter_provider;
pub mod logging;
pub mod login_info;
pub mod message_event_type_filter;
pub mod message_helpers;
pub mod message_sender;
pub mod models;
pub mod multimodal_image_url;
pub mod natural_language_reply;
pub mod profile;
pub mod send_friend_message_batches;
pub mod send_group_message_batches;
pub mod send_message;
pub mod send_qq_message_batches;
pub mod system_config;
pub mod tools;
pub mod utils;
pub mod ws_action;

pub use active_adapter_manager::{
    close_runtime_bot_adapter_instance, ensure_active_bot_adapter, get_active_bot_adapter_handle,
    has_active_bot_adapter, initialize_enabled_bot_adapters, list_active_bot_adapter_connection_ids,
    list_runtime_bot_adapter_instances, register_active_bot_adapter, stop_active_bot_adapter,
    sync_enabled_bot_adapters,
};
pub use extract_optional_group_id_from_event::ExtractOptionalGroupIdFromEventNode;
pub use extract_qq_message_list_from_event::ExtractQQMessageListFromEventNode;
pub use extract_sender_from_event::ExtractSenderFromEventNode;
pub use extract_sender_id_from_event::ExtractSenderIdFromEventNode;
pub use ims_bot_adapter_provider::ImsBotAdapterProviderNode;
pub use login_info::{fetch_login_info, fetch_login_info_via_adapter_connection, qq_avatar_url};
pub use message_event_type_filter::MessageEventTypeFilterNode;
pub use message_sender::MessageSenderNode;
pub use profile::{
    profile_from_login_info, resolve_active_or_fallback_bot_profile,
    resolve_active_or_fallback_bot_profile_from_connection, resolve_fallback_bot_profile,
    resolve_fallback_bot_profile_from_connection, QqBotProfile,
};
pub use send_friend_message_batches::SendFriendMessageBatchesNode;
pub use send_group_message_batches::SendGroupMessageBatchesNode;
pub use send_message::SendMessageNode;
pub use send_qq_message_batches::SendQQMessageBatchesNode;
pub use system_config::{
    build_ims_bot_adapter, load_ims_bot_adapter_connections, parse_ims_bot_adapter_connection,
    save_ims_bot_adapter_connections, BotAdapterConnection, BotAdapterConnectionConfig, BotAdapterConnectionKind,
    BotAdapterConnectionsSection,
};

use crate::ims_bot_adapter::models::message::Message;
use crate::error::Result;

/// Opaque handle for the bot adapter, stored in DataValue.
/// The concrete type is `Arc<TokioMutex<BotAdapter>>` in the main crate;
/// it is type-erased here so that downstream crates can hold it without
/// depending on a concrete adapter implementation.
pub type BotAdapterHandle = std::sync::Arc<dyn std::any::Any + Send + Sync + 'static>;

#[macro_export]
macro_rules! sender_display_name {
    ($sender_name:expr, $sender_card:expr) => {{
        let card = $sender_card.trim();
        if card.is_empty() {
            $sender_name.to_string()
        } else {
            card.to_string()
        }
    }};
}
pub use crate::sender_display_name;

// Labels for message structure elements used when rendering ims messages
pub const CURRENT_MESSAGE_LABEL: &str = "[Current Message]";
pub const REPLY_MESSAGE_LABEL: &str = "[Reply Message]";
pub const FORWARD_NODE_LABEL: &str = "[Forward Node]";
pub const SENDER_LABEL: &str = "[Sender]";

// Text markers used to delimit nested message structures
pub const REPLY_START_MARKER: &str = "[Reply Message Start]";
pub const REPLY_END_MARKER: &str = "[Reply Message End]";
pub const FORWARD_START_MARKER: &str = "[Forward Message Start]";
pub const FORWARD_END_MARKER: &str = "[Forward Message End]";
pub const NOT_ANY_TEXT_MARKER: &str = "[No Text Content]";
pub const NOT_REPLAY_TEXT_MARKER: &str = "[No Reply]";

// Labels for message content sections used in LLM prompts
pub const REPLAY_CONTENT_LABEL: &str = "[Replay Content]";
pub const FORWARD_CONTENT_LABEL: &str = "[Forward Content]";
pub const IMAGE_ANALYSIS_LABEL: &str = "[Image Analysis]";
pub const QUOTE_CONTENT_APPENDIX_LABEL: &str = "[Quote Content Appendix]";

/// Checks whether any message in the list carries actual content (text, image, or
/// recursively meaningful reply/forward content). A depth limit of 8 prevents
/// infinite recursion from deeply nested or cyclic structures.
pub fn messages_have_effective_content(messages: &[Message], depth: usize) -> bool {
    if depth > 8 {
        return false;
    }

    for message in messages {
        match message {
            Message::PlainText(plain) => {
                if !plain.text.trim().is_empty() {
                    return true;
                }
            }
            Message::Image(_) => return true,
            Message::Forward(forward) => {
                if forward
                    .content
                    .iter()
                    .any(|node| messages_have_effective_content(&node.content, depth + 1))
                {
                    return true;
                }
            }
            Message::Reply(reply) => {
                if let Some(source_messages) = reply.message_source.as_deref() {
                    if matches!(source_messages, [Message::Reply(_)]) {
                        continue;
                    }
                    if messages_have_effective_content(source_messages, depth + 1) {
                        return true;
                    }
                }
            }
            Message::At(_) => {}
        }
    }

    false
}

pub fn init_node_registry() -> Result<()> {
    use crate::register_node;

    use extract_group_id_from_event::ExtractGroupIdFromEventNode;
    use extract_message_from_event::ExtractMessageFromEventNode;
    use extract_optional_group_id_from_event::ExtractOptionalGroupIdFromEventNode;
    use extract_qq_message_list_from_event::ExtractQQMessageListFromEventNode;
    use ims_bot_adapter_provider::ImsBotAdapterProviderNode;

    register_node!(
        "ims_bot_adapter_provider",
        "IMS BotAdapter Provider",
        "Bot适配器",
        "从系统连接配置中选择已启用的 IMS Bot Adapter 并输出 BotAdapterRef 引用",
        ImsBotAdapterProviderNode
    );
    register_node!(
        "send_message",
        "发送消息",
        "Bot适配器",
        "根据 Sender 向 QQ 好友或群组发送消息",
        SendMessageNode
    );
    register_node!(
        "send_friend_message_batches",
        "批量发送好友消息",
        "Bot适配器",
        "向QQ好友逐批发送 Vec<Vec<QQMessage>>，支持两次发送之间延迟",
        SendFriendMessageBatchesNode
    );
    register_node!(
        "send_group_message_batches",
        "批量发送群组消息",
        "Bot适配器",
        "向QQ群组逐批发送 Vec<Vec<QQMessage>>，支持两次发送之间延迟",
        SendGroupMessageBatchesNode
    );
    register_node!(
        "send_qq_message_batches",
        "发送QQ消息批次",
        "Bot适配器",
        "将 QQ 消息批次逐批发送到好友或群组，并输出发送汇总",
        SendQQMessageBatchesNode
    );
    register_node!(
        "extract_message_from_event",
        "事件提取 LLMMessage 列表",
        "Bot适配器",
        "从消息事件中提取 LLMMessage 列表",
        ExtractMessageFromEventNode
    );
    register_node!(
        "extract_qq_message_list_from_event",
        "事件提取 QQMessage 列表",
        "Bot适配器",
        "从消息事件中提取原始 QQ 消息列表 (Vec<QQMessage>)",
        ExtractQQMessageListFromEventNode
    );
    register_node!(
        "extract_sender_from_event",
        "提取发送者",
        "Bot适配器",
        "从消息事件中提取可用于回发的 Sender",
        ExtractSenderFromEventNode
    );
    register_node!(
        "message_event_type_filter",
        "消息类型分支",
        "Bot适配器",
        "根据消息类型（好友/群组）路由消息事件",
        MessageEventTypeFilterNode
    );
    register_node!(
        "extract_sender_id_from_event",
        "提取发送者ID",
        "Bot适配器",
        "从消息事件中提取发送者的QQ号（字符串）",
        ExtractSenderIdFromEventNode
    );
    register_node!(
        "extract_group_id_from_event",
        "提取群号",
        "Bot适配器",
        "从群消息事件中提取群号（字符串）",
        ExtractGroupIdFromEventNode
    );
    register_node!(
        "extract_optional_group_id_from_event",
        "提取可选群号",
        "Bot适配器",
        "从消息事件中提取群号；私聊时返回空字符串",
        ExtractOptionalGroupIdFromEventNode
    );

    Ok(())
}