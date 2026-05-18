pub mod agent;
pub mod nodes;
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
    use nodes::agent_tavily_ref::AgentTavilyRefNode;
    use nodes::brain_node::BrainNode;

    register_node!(
        "agent_llm",
        "读取Agent LLM",
        "Agent",
        "从当前 Agent 工具调用上下文中读取主模型、意图分类模型或数学编程模型，并输出 LLModel 引用",
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
        "读取Agent Tavily连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取 Tavily 连接并输出 TavilyRef",
        AgentTavilyRefNode
    );

    Ok(())
}
