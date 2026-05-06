pub mod adapter;
pub mod event;
pub mod extract_group_id_from_event;
pub mod extract_message_from_event;
pub mod extract_qq_message_list_from_event;
pub mod extract_sender_id_from_event;
pub mod message_event_type_filter;
pub mod message_helpers;
pub mod message_sender;
pub mod models;
pub mod send_friend_message;
pub mod send_friend_message_batches;
pub mod send_group_message;
pub mod send_group_message_batches;
pub mod send_qq_message_batches;
pub mod system_config;
pub mod ws_action;

use zihuan_core::error::Result;
use zihuan_graph_engine::register_node;

pub use extract_qq_message_list_from_event::ExtractQQMessageListFromEventNode;
pub use extract_sender_id_from_event::ExtractSenderIdFromEventNode;
pub use message_event_type_filter::MessageEventTypeFilterNode;
pub use message_sender::MessageSenderNode;
pub use send_friend_message::SendFriendMessageNode;
pub use send_friend_message_batches::SendFriendMessageBatchesNode;
pub use send_group_message::SendGroupMessageNode;
pub use send_group_message_batches::SendGroupMessageBatchesNode;
pub use send_qq_message_batches::SendQQMessageBatchesNode;
pub use system_config::{
    build_ims_bot_adapter, load_ims_bot_adapter_connections, save_ims_bot_adapter_connections,
    parse_ims_bot_adapter_connection, BotAdapterConnection, BotAdapterConnectionConfig,
    BotAdapterConnectionKind, BotAdapterConnectionsSection,
};

pub fn init_node_registry() -> Result<()> {
    use extract_group_id_from_event::ExtractGroupIdFromEventNode;
    use extract_message_from_event::ExtractMessageFromEventNode;
    use extract_qq_message_list_from_event::ExtractQQMessageListFromEventNode;

    register_node!(
        "send_friend_message",
        "发送好友消息",
        "Bot适配器",
        "向QQ好友发送消息",
        SendFriendMessageNode
    );
    register_node!(
        "send_group_message",
        "发送群组消息",
        "Bot适配器",
        "向QQ群组发送消息",
        SendGroupMessageNode
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
        "extract_qq_message_list_from_event",
        "事件提取 QQMessage 列表",
        "Bot适配器",
        "从消息事件中提取原始 QQ 消息列表 (Vec<QQMessage>)",
        ExtractQQMessageListFromEventNode
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

    Ok(())
}
