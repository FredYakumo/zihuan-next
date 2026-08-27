import { port } from "../graph_engine/zihuan_sdk.mjs";

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "text_embedding", display_name: "文本向量化", category: "AI", description: "使用 EmbeddingModel 将文本编码为向量",
    input_ports: [port("embedding_model", "EmbeddingModel"), port("text", "String")], output_ports: [port("embedding", "Vector"), port("dimension", "Integer")],
    execute: async ({ inputs, sdk }) => sdk.embeddings.infer(inputs.embedding_model, inputs.text),
  },
  {
    type_id: "batch_text_embedding", display_name: "批量文本向量化", category: "AI", description: "使用 EmbeddingModel 批量将文本编码为向量",
    input_ports: [port("embedding_model", "EmbeddingModel"), port("texts", { Vec: "String" })], output_ports: [port("embeddings", { Vec: "Vector" }), port("count", "Integer"), port("dimension", "Integer")],
    execute: async ({ inputs, sdk }) => sdk.embeddings.batchInfer(inputs.embedding_model, inputs.texts),
  },
];
