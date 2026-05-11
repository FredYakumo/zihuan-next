pub mod agent;
pub mod agent_config_nodes;
pub mod agent_text_similarity;
pub mod brain_node;
pub mod brain_tool;
pub mod inference_function;
pub mod linalg;
pub mod llm_api;
pub mod llm_api_node;
pub mod llm_base;
pub mod llm_infer_node;
pub mod model;
pub mod nn;
pub mod prompt;
pub mod rag;
pub mod system_config;
pub mod tool_subgraph;
pub mod tooling;
pub mod util;

use zihuan_core::error::Result;
use zihuan_graph_engine::register_node;

pub use model::{InferenceParam, MessageRole, OpenAIMessage};
pub use util::{role_to_str, str_to_role, SystemMessage, UserMessage};

pub fn init_node_registry() -> Result<()> {
    use agent_config_nodes::agent_embedding_model_node::AgentEmbeddingModelNode;
    use agent_config_nodes::agent_image_db_ref_node::AgentImageDbRefNode;
    use agent_config_nodes::agent_llm_node::AgentLlmNode;
    use agent_config_nodes::agent_mysql_ref_node::AgentMySqlRefNode;
    use agent_config_nodes::agent_rustfs_ref_node::AgentRustfsRefNode;
    use agent_config_nodes::agent_tavily_ref_node::AgentTavilyRefNode;
    use brain_node::BrainNode;
    use inference_function::compact_message::ContextCompactNode;
    use linalg::batch_text_embedding_node::BatchTextEmbeddingNode;
    use linalg::embedding_api_node::LoadTextEmbedderNode;
    use linalg::text_embedding_node::TextEmbeddingNode;
    use linalg::top_k_similarity_node::TopKSimilarityNode;
    use linalg::vector_cosine_similarity_node::VectorCosineSimilarityNode;
    use llm_api_node::LLMApiNode;
    use llm_infer_node::LLMInferNode;
    use nn::local_candle_embedding_node::LoadLocalTextEmbedderNode;
    use rag::tavily_provider_node::TavilyProviderNode;
    use rag::tavily_search_node::TavilySearchNode;
    use rag::weaviate_image_search_node::WeaviateImageSearchNode;

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
        "agent_tavily_ref",
        "读取Agent Tavily连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取 Tavily 连接并输出 TavilyRef",
        AgentTavilyRefNode
    );
    register_node!(
        "agent_image_db_ref",
        "读取Agent图片库连接",
        "Agent",
        "从当前 Agent 工具调用上下文中读取图片向量库连接并输出 WeaviateRef",
        AgentImageDbRefNode
    );
    register_node!(
        "llm_api",
        "llm配置",
        "AI",
        "配置语言模型连接，输出LLModel引用",
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
        "load_text_embedder",
        "加载文本Embedder(API)",
        "AI",
        "加载远程文本 embedding API 配置，输出 EmbeddingModel 引用",
        LoadTextEmbedderNode
    );
    register_node!(
        "load_local_text_embedder",
        "加载文本Embedder(本地)",
        "AI",
        "从 models/text_embedding 目录加载本地 Candle embedding 模型，输出 EmbeddingModel 引用",
        LoadLocalTextEmbedderNode
    );
    register_node!(
        "text_embedding",
        "文本向量化",
        "AI",
        "使用 EmbeddingModel 将文本编码为向量",
        TextEmbeddingNode
    );
    register_node!(
        "batch_text_embedding",
        "批量文本向量化",
        "AI",
        "使用 EmbeddingModel 批量将文本编码为向量",
        BatchTextEmbeddingNode
    );
    register_node!(
        "vector_cosine_similarity",
        "向量余弦相似度",
        "AI",
        "使用 general-wheel-cpp 计算两个向量的余弦相似度",
        VectorCosineSimilarityNode
    );
    register_node!(
        "top_k_similarity",
        "Top-K相似检索",
        "AI",
        "对 Vec<Vector> 与查询向量执行 top-k 相似度检索",
        TopKSimilarityNode
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
    register_node!(
        "weaviate_image_search",
        "Weaviate 图片检索",
        "AI",
        "使用本地 Weaviate 图片库做语义检索，输出标准化图片结果 JSON",
        WeaviateImageSearchNode
    );

    Ok(())
}
