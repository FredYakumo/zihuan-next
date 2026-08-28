import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "agent_llm", display_name: "读取Agent LLM", category: "Agent", description: "从当前 Agent 工具调用上下文中读取指定 LLM，并输出 LLModel 引用",
    config_fields: [{ key: "llm_kind", data_type: "String", description: "选择读取主模型、数学编程模型或自然语言回复模型", required: true, widget: "agent_llm_kind_select" }], input_ports: [], output_ports: [port("llm_model", "LLModel")],
    execute: async ({ inline_values, zihuan }) => ({ llm_model: await zihuan.agent.llm(inline_values.llm_kind) }),
  },
  {
    type_id: "agent_embedding_model", display_name: "读取Agent文本向量模型", category: "Agent", description: "从当前 Agent 工具调用上下文中读取文本向量模型并输出 EmbeddingModel 引用",
    input_ports: [], output_ports: [port("embedding_model", "EmbeddingModel")],
    execute: async ({ zihuan }) => ({ embedding_model: await zihuan.agent.embeddingModel() }),
  },
  {
    type_id: "agent_tool_task", display_name: "读取Agent工具任务", category: "工具调用", description: "读取当前 Agent 工具调用关联的任务 ID 与是否存在任务",
    input_ports: [], output_ports: [port("task_id", "String"), port("has_task", "Boolean")],
    execute: async ({ zihuan }) => zihuan.agent.task(),
  },
  {
    type_id: "agent_task_progress", display_name: "更新Agent任务进度", category: "工具调用", description: "向任务追加一条进度消息",
    input_ports: [port("task_id", "String"), port("message", "String")], output_ports: [port("ok", "Boolean")],
    execute: async ({ inputs, zihuan }) => ({ ok: await zihuan.task.append(inputs.task_id, inputs.message) }),
  },
  {
    type_id: "agent_rdb_ref", display_name: "读取Agent RDB连接", category: "Agent", description: "从当前 Agent 工具调用上下文中读取关系数据库连接并输出 RdbRef",
    input_ports: [], output_ports: [port("rdb_ref", "RdbRef")],
    execute: async ({ zihuan }) => ({ rdb_ref: await zihuan.agent.rdb() }),
  },
  {
    type_id: "agent_rustfs_ref", display_name: "读取Agent RustFS连接", category: "Agent", description: "从当前 Agent 工具调用上下文中读取 RustFS 连接并输出 S3Ref",
    input_ports: [], output_ports: [port("s3_ref", "S3Ref")],
    execute: async ({ zihuan }) => ({ s3_ref: await zihuan.agent.s3() }),
  },
  {
    type_id: "agent_image_db_ref", display_name: "读取Agent图片库连接", category: "Agent", description: "从当前 Agent 工具调用上下文中读取图片向量库连接并输出 WeaviateRef",
    input_ports: [], output_ports: [port("weaviate_ref", "WeaviateRef")],
    execute: async ({ zihuan }) => ({ weaviate_ref: await zihuan.agent.imageWeaviate() }),
  },
  {
    type_id: "agent_tavily_ref", display_name: "读取Agent Web Search Engine连接", category: "Agent", description: "从当前 Agent 工具调用上下文中读取 Web Search Engine 连接并输出 WebSearchEngineRef",
    input_ports: [], output_ports: [port("web_search_engine_ref", "WebSearchEngineRef")],
    execute: async ({ zihuan }) => ({ web_search_engine_ref: await zihuan.agent.webSearch() }),
  },
];
