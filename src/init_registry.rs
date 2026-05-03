use crate::error::Result;
use zihuan_node::register_node;

/// Initialize all node types in the registry.
/// Delegates base node registrations to `zihuan_node::registry::init_node_registry` and
/// then registers bot-adapter and LLM nodes that live in the main crate.
pub fn init_node_registry() -> Result<()> {
    // Register all nodes from the zihuan_node crate.
    zihuan_node::registry::init_node_registry()?;

    use zihuan_bot_adapter::extract_group_id_from_event::ExtractGroupIdFromEventNode;
    use zihuan_bot_adapter::extract_message_from_event::ExtractMessageFromEventNode;
    use zihuan_bot_adapter::extract_qq_message_list_from_event::ExtractQQMessageListFromEventNode;
    use zihuan_bot_adapter::{
        BotAdapterNode, ExtractSenderIdFromEventNode, MessageEventTypeFilterNode,
        SendFriendMessageBatchesNode, SendFriendMessageNode, SendGroupMessageBatchesNode,
        SendGroupMessageNode, SendQQMessageBatchesNode,
    };
    use zihuan_llm::agent::qq_message_agent_node::QqMessageAgentNode;
    use zihuan_llm::brain_node::BrainNode;
    use zihuan_llm::context_compact_node::ContextCompactNode;
    use zihuan_llm::llm_api_node::LLMApiNode;
    use zihuan_llm::llm_infer_node::LLMInferNode;
    use zihuan_llm::rag::tavily_provider_node::TavilyProviderNode;
    use zihuan_llm::rag::tavily_search_node::TavilySearchNode;

    // LLM nodes
    register_node!(
        "llm_api",
        "LLM API配置",
        "AI",
        "配置语言模型API连接，输出LLModel引用",
        LLMApiNode
    );

    register_node!(
        "llm_infer",
        "LLM推理",
        "AI",
        "使用LLModel引用对消息列表进行一次推理",
        LLMInferNode
    );

    register_node!(
        "brain",
        "Brain",
        "AI",
        "使用 LLM + system prompt + user message 触发带可编辑 Tools 的函数调用推理",
        BrainNode
    );

    register_node!(
        "context_compact",
        "上下文压缩",
        "AI",
        "压缩 OpenAIMessage 历史，仅保留摘要对和最近 2 条非 tool 消息",
        ContextCompactNode
    );

    register_node!(
        "tavily_provider",
        "Tavily Provider",
        "AI",
        "配置 Tavily 搜索 API Token，输出 TavilyRef 引用",
        TavilyProviderNode
    );

    register_node!(
        "tavily_search",
        "Tavily 搜索",
        "AI",
        "使用 TavilyRef 执行 Tavily 搜索并输出包含标题、链接和内容的 Vec<String>",
        TavilySearchNode
    );

    // Bot adapter nodes
    register_node!(
        "qq_message_agent",
        "QQ Message Agent",
        "Bot适配器",
        "使用Brain智能体响应消息事件，智能体会结合自身状态对消息事件进行判断并做出响应。",
        QqMessageAgentNode
    );

    register_node!(
        "bot_adapter",
        "QQ机器人适配器",
        "Bot适配器",
        "接收来自QQ服务器的消息事件",
        BotAdapterNode
    );

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
