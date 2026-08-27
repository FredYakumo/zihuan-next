import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "qq_message_list_weaviate_persistence", display_name: "QQMessage列表向量持久化", category: "消息存储", description: "将Vec<QQMessage>及调用方提供的元数据向量化后持久化到Weaviate数据库",
    input_ports: [port("qq_message_list", { Vec: "QQMessage" }), port("message_id", "String"), port("sender_id", "String"), port("sender_name", "String"), port("group_id", "String", { required: false }), port("group_name", "String", { required: false }), port("weaviate_ref", "WeaviateRef"), port("embedding_model", "EmbeddingModel")],
    output_ports: [port("success", "Boolean"), port("qq_message_list", { Vec: "QQMessage" })],
    execute: async ({ inputs, zihuan }) => ({ success: await zihuan.storage.persistQQMessageVectors(zihuan.resources.weaviate(inputs.weaviate_ref), zihuan.resources.embeddingModel(inputs.embedding_model), inputs.qq_message_list, inputs), qq_message_list: inputs.qq_message_list }),
  },
  {
    type_id: "image_weaviate_persistence", display_name: "图片向量持久化", category: "消息存储", description: "将对象存储路径、图片总结与向量持久化到Weaviate数据库",
    input_ports: [port("object_storage_path", "String"), port("description", "String"), port("weaviate_ref", "WeaviateRef"), port("embedding_model", "EmbeddingModel", { required: false }), port("vector", "Vector", { required: false }), port("source", "String", { required: false }), port("media_id", "String", { required: false }), port("original_source", "String", { required: false }), port("name", "String", { required: false }), port("mime_type", "String", { required: false })],
    output_ports: [port("success", "Boolean"), port("object_storage_path", "String")],
    execute: async ({ inputs, zihuan }) => ({ success: await zihuan.storage.persistImageVector(zihuan.resources.weaviate(inputs.weaviate_ref), { ...inputs, embedding_model: inputs.embedding_model ? zihuan.resources.embeddingModel(inputs.embedding_model) : undefined }), object_storage_path: inputs.object_storage_path }),
  },
  {
    type_id: "weaviate_image_search", display_name: "Weaviate 图片检索", category: "AI", description: "使用本地 Weaviate 图片库做语义检索，输出标准化图片结果 JSON",
    input_ports: [port("weaviate_ref", "WeaviateRef"), port("embedding_model", "EmbeddingModel"), port("query", "String"), port("limit", "Integer"), port("max_distance", "Float", { required: false }), port("target_vector", "String", { required: false })],
    output_ports: [port("images", "Json"), port("has_results", "Boolean")],
    execute: async ({ inputs, zihuan }) => zihuan.storage.searchImages(zihuan.resources.weaviate(inputs.weaviate_ref), zihuan.resources.embeddingModel(inputs.embedding_model), inputs.query, { limit: inputs.limit, max_distance: inputs.max_distance, target_vector: inputs.target_vector }),
  },
];
