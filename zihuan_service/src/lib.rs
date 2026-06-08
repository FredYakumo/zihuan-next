pub mod agent;
pub mod command;
pub mod nodes;
pub mod qq_chat_user_input;
mod resource_resolver;
pub mod scheduled_task;
pub mod storage;

pub use agent::{AgentManager, AgentRuntimeInfo, AgentRuntimeStatus};

use zihuan_core::error::Result;

pub fn init_node_registry() -> Result<()> {
    use zihuan_graph_engine::register_node;

    use nodes::agent_embedding_model_node::AgentEmbeddingModelNode;
    use nodes::agent_image_db_ref::AgentImageDbRefNode;
    use nodes::agent_llm_node::AgentLlmNode;
    use nodes::agent_mysql_ref::AgentMySqlRefNode;
    use nodes::agent_rustfs_ref::AgentRustfsRefNode;
    use nodes::agent_task_progress_node::AgentTaskProgressNode;
    use nodes::agent_tavily_ref::AgentTavilyRefNode;
    use nodes::agent_tool_task_node::AgentToolTaskNode;
    use nodes::brain_node::BrainNode;
    use nodes::tavily_web_search::TavilyWebSearchNode;

    register_node!(
        "agent_llm",
        "读取Agent LLM",
        "Agent",
        "从当前 Agent 工具调用上下文中读取主模型、数学编程模型或自然语言回复模型，并输出 LLModel 引用",
        AgentLlmNode
    );
    register_node!(
        "agent_embedding_model",
        "读取Agent文本向量模型",
        "Agent",
        "从当前 Agent 工具调用上下文中读取文本向量模型并输出 EmbeddingModel 引用",
        AgentEmbeddingModelNode
    );
    register_node!(
        "agent_tool_task",
        "读取Agent工具任务",
        "工具调用",
        "读取当前 Agent 工具调用关联的任务 ID 与是否存在任务",
        AgentToolTaskNode
    );
    register_node!(
        "agent_task_progress",
        "更新Agent任务进度",
        "工具调用",
        "向任务追加一条进度消息",
        AgentTaskProgressNode
    );
    register_node!(
        "brain",
        "Brain",
        "AI",
        "使用 LLM + system prompt + user message 触发带可编辑 Tools 的函数调用推理",
        BrainNode
    );
    register_node!(
        "agent_mysql_ref",
        "读取Agent MySQL连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取 MySQL 连接并输出 MySqlRef",
        AgentMySqlRefNode
    );
    register_node!(
        "agent_rustfs_ref",
        "读取Agent RustFS连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取 RustFS 连接并输出 S3Ref",
        AgentRustfsRefNode
    );
    register_node!(
        "agent_image_db_ref",
        "读取Agent图片库连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取图片向量库连接并输出 WeaviateRef",
        AgentImageDbRefNode
    );
    register_node!(
        "agent_tavily_ref",
        "读取Agent Web Search Engine连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取 Web Search Engine 连接并输出 WebSearchEngineRef",
        AgentTavilyRefNode
    );
    register_node!(
        "tavily_web_search",
        "网页搜索",
        "工具",
        "使用 Web Search Engine 搜索网页，或对单个 URL 抽取正文内容",
        TavilyWebSearchNode
    );

    Ok(())
}
