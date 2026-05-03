# QQ Agent Similarity

本文说明 `QQ Message Agent` 在最终回复判定阶段，如何使用文本向量、BM25 和 `general_wheel_cpp` 线性代数加速库完成混合相似度检查。

---

## Overview

相关实现位于：

- `packages/zihuan_llm/src/agent/qq_message_agent_node.rs`
- `packages/zihuan_llm/src/agent_text_similarity.rs`
- `packages/general_wheel_cpp/src/lib.rs`

`QQ Message Agent` 在最终 assistant 文本准备发送前，会进入一条统一判定管线：

1. 基础清洗
2. 规则硬拦截
3. 混合相似度检查
4. 长文本转 forward
5. 发送或丢弃

混合相似度检查用于两类场景：

- 检测最终 assistant 是否与当前待发送消息或近期历史回复近重复
- 检测最终 assistant 是否接近一组“坏样本模板”，例如“已完成回复”“处理结果如下”

---

## Runtime Flow

入口函数：

- `decide_final_assistant_send(...)`

当最终 assistant 文本长度达到最小相似度检查阈值后，系统会构造候选集并调用：

- `find_best_match(...)`

候选集分为两类：

1. 近重复候选
   - 当前轮 `reply_*` 工具已生成的可文本化消息
   - 最近几轮历史中的 assistant 文本

2. 坏样本候选
   - 代码内置的 `BAD_REPLY_SAMPLES`

返回的最佳候选会带上：

- `bm25_score`
- `bm25_normalized`
- `cosine_score`
- `hybrid_score`

随后 `QQ Message Agent` 使用阈值决定：

- 丢弃最终 assistant
- 允许发送
- 或先转成 forward 再发送

---

## Linear Algebra Usage

向量相似度计算使用 `general_wheel_cpp`，而不是在 agent 内手写余弦公式。

当前使用点：

- `packages/zihuan_llm/src/agent_text_similarity.rs`

具体调用：

```rust
use general_wheel_cpp::cosine_similarity;
```

在 `find_best_match(...)` 中：

1. 先通过 `EmbeddingModel::batch_inference(...)` 获取查询文本和候选文本的 embedding
2. 取第一条 embedding 作为 query
3. 对其余候选 embedding 逐条调用 `general_wheel_cpp::cosine_similarity(...)`
4. 将结果作为 `cosine_score` 参与混合打分

这样做的原因：

- 向量点积和范数计算交给底层加速库
- 避免在 agent 逻辑中重复维护一套手写线性代数实现
- 保持和现有 `VectorCosineSimilarityNode`、`TopKSimilarityNode` 的实现方向一致

当前 `QQ Agent` 还没有直接使用 `general_wheel_cpp::top_k_similar`；目前只复用了加速余弦相似度，候选遍历与 BM25 组合逻辑仍在 Rust 层完成。

---

## Embedding Model Requirement

向量相似度是 **可选增强**，只有在 `QQ Message Agent` 接入 `embedding_model` 输入后才会启用。

节点端口：

- `embedding_model: EmbeddingModel`（optional）

接线方式：

1. 使用 `load_text_embedder` 节点创建 `EmbeddingModel`
2. 将其输出连接到 `qq_message_agent.embedding_model`

如果未提供 `embedding_model`：

- 规则硬拦截仍然生效
- BM25 仍然生效
- `cosine_score` 为 `None`
- `hybrid_score` 会退化为仅基于 BM25 的结果

这保证旧图不需要立刻迁移，也不会因为缺少 embedding 模型而失效。

---

## Related Nodes

与本文相关的节点和能力：

- `load_text_embedder`
- `text_embedding`
- `batch_text_embedding`
- `vector_cosine_similarity`
- `top_k_similarity`

其中：

- `vector_cosine_similarity` 使用 `general_wheel_cpp::cosine_similarity`
- `top_k_similarity` 使用 `general_wheel_cpp::top_k_similar`

`QQ Message Agent` 当前没有通过节点图来执行这些能力，而是直接在 Rust 运行时内部调用相似度 helper。这是为了避免把最终回复判定拆成额外图执行步骤。

---

## Notes

- `general_wheel_cpp` 只负责向量相似度这部分数值计算，不负责 BM25。
- BM25 当前由 `agent_text_similarity.rs` 内部实现。
- 如果后续需要进一步加速大候选集场景，可以考虑在保持当前接口不变的前提下，把候选排序进一步切换到 `top_k_similar`。
