import { port } from "../graph_engine/zihuan_sdk.mjs";

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "load_text_embedder", display_name: "加载文本Embedder(API)", category: "AI", description: "加载远程文本 embedding API 配置，输出 EmbeddingModel 引用",
    input_ports: [port("model_name", "String"), port("api_endpoint", "String"), port("api_key", "Password", { required: false }), port("timeout_secs", "Integer", { required: false }), port("retry_count", "Integer", { required: false })], output_ports: [port("embedding_model", "EmbeddingModel")],
    execute: async ({ inputs, sdk }) => ({ embedding_model: await sdk.embeddings.createRemote(inputs) }),
  },
  {
    type_id: "load_local_text_embedder", display_name: "加载文本Embedder(本地)", category: "AI", description: "从 models/text_embedding 目录加载本地 Candle embedding 模型，输出 EmbeddingModel 引用",
    input_ports: [port("model_name", "String")], output_ports: [port("embedding_model", "EmbeddingModel")],
    execute: async ({ inputs, sdk }) => ({ embedding_model: await sdk.embeddings.createLocal(inputs.model_name) }),
  },
];
