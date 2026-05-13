pub mod agent_config_support;
pub mod inference_function;
pub mod linalg;
pub mod llm_api;
pub mod nn;
pub mod nodes;
pub mod system_config;

use zihuan_core::error::Result;

pub fn init_node_registry() -> Result<()> {
    use zihuan_graph_engine::register_node;

    use nodes::agent_embedding_model_node::AgentEmbeddingModelNode;
    use nodes::agent_llm_node::AgentLlmNode;
    use nodes::batch_text_embedding_node::BatchTextEmbeddingNode;
    use nodes::context_compact_node::ContextCompactNode;
    use nodes::llm_infer_node::LLMInferNode;
    use nodes::llm_node::LlmNode;
    use nodes::load_local_text_embedder_node::LoadLocalTextEmbedderNode;
    use nodes::load_text_embedder_node::LoadTextEmbedderNode;
    use nodes::text_embedding_node::TextEmbeddingNode;
    use nodes::top_k_similarity_node::TopKSimilarityNode;
    use nodes::vector_cosine_similarity_node::VectorCosineSimilarityNode;

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
        "llm_api",
        "llm配置",
        "AI",
        "配置语言模型连接，输出LLModel引用",
        LlmNode
    );
    register_node!(
        "llm_infer",
        "LLM推理",
        "AI",
        "使用LLModel引用对消息列表进行一次推理",
        LLMInferNode
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

    Ok(())
}
