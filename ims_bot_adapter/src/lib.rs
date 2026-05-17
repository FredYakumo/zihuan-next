pub mod active_adapter_manager;
pub mod adapter;
pub mod event;
pub mod extract_group_id_from_event;
pub mod extract_message_by_id_from_event;
pub mod extract_message_from_event;
pub mod extract_optional_group_id_from_event;
pub mod extract_qq_message_list_from_event;
pub mod extract_sender_from_event;
pub mod extract_sender_id_from_event;
pub mod ims_bot_adapter_provider;
pub mod login_info;
pub mod message_event_type_filter;
pub mod message_helpers;
pub mod message_sender;
pub mod models;
pub mod send_friend_message_batches;
pub mod send_group_message_batches;
pub mod send_message;
pub mod send_qq_message_batches;
pub mod system_config;
pub mod ws_action;

use zihuan_core::error::Result;
use zihuan_graph_engine::register_node;

pub use active_adapter_manager::{
    close_runtime_bot_adapter_instance, ensure_active_bot_adapter, get_active_bot_adapter_handle,
    has_active_bot_adapter, initialize_enabled_bot_adapters,
    list_active_bot_adapter_connection_ids, list_runtime_bot_adapter_instances,
    register_active_bot_adapter, stop_active_bot_adapter, sync_enabled_bot_adapters,
};
pub use extract_message_by_id_from_event::ExtractMessageByIdFromEventNode;
pub use extract_optional_group_id_from_event::ExtractOptionalGroupIdFromEventNode;
pub use extract_qq_message_list_from_event::ExtractQQMessageListFromEventNode;
pub use extract_sender_from_event::ExtractSenderFromEventNode;
pub use extract_sender_id_from_event::ExtractSenderIdFromEventNode;
pub use ims_bot_adapter_provider::ImsBotAdapterProviderNode;
pub use login_info::{fetch_login_info, qq_avatar_url};
pub use message_event_type_filter::MessageEventTypeFilterNode;
pub use message_sender::MessageSenderNode;
pub use send_friend_message_batches::SendFriendMessageBatchesNode;
pub use send_group_message_batches::SendGroupMessageBatchesNode;
pub use send_message::SendMessageNode;
pub use send_qq_message_batches::SendQQMessageBatchesNode;
pub use system_config::{
    build_ims_bot_adapter, load_ims_bot_adapter_connections, parse_ims_bot_adapter_connection,
    save_ims_bot_adapter_connections, BotAdapterConnection, BotAdapterConnectionConfig,
    BotAdapterConnectionKind, BotAdapterConnectionsSection,
};

pub fn init_node_registry() -> Result<()> {
    use extract_group_id_from_event::ExtractGroupIdFromEventNode;
    use extract_message_by_id_from_event::ExtractMessageByIdFromEventNode;
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
        "事件提取 OpenAIMessage 列表",
        "Bot适配器",
        "从消息事件中提取 OpenAIMessage 列表",
        ExtractMessageFromEventNode
    );
    register_node!(
        "extract_message_by_id_from_event",
        "按消息ID提取 OpenAIMessage 列表",
        "Bot适配器",
        "从当前消息事件或指定消息ID恢复消息，并提取 OpenAIMessage 列表",
        ExtractMessageByIdFromEventNode
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
