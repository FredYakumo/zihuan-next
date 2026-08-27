import { port } from "../graph_engine/zihuan_sdk.mjs";

const connectionField = { key: "config_id", data_type: "String", description: "选择系统中的 Web Search Engine 连接配置", required: true, widget: "connection_select", connection_kind: "web_search_engine" };

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "tavily_provider", display_name: "Web Search Engine Provider", category: "AI", description: "从系统连接中选择 Web Search Engine 配置，输出 WebSearchEngineRef 引用",
    config_fields: [connectionField], input_ports: [], output_ports: [port("tavily_ref", "WebSearchEngineRef")],
    execute: async ({ inline_values, sdk }) => ({ tavily_ref: await sdk.search.createProvider(inline_values.config_id ?? inline_values.connection_id) }),
  },
  {
    type_id: "tavily_search", display_name: "网页搜索", category: "AI", description: "使用 WebSearchEngineRef 执行网页搜索并输出包含标题、链接和内容的 Vec<String>",
    input_ports: [port("tavily_ref", "WebSearchEngineRef"), port("query", "String"), port("search_count", "Integer")], output_ports: [port("results", { Vec: "String" })],
    execute: async ({ inputs, sdk }) => sdk.search.query(inputs.tavily_ref, inputs.query, inputs.search_count),
  },
];
