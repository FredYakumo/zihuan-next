import { port } from "../graph_engine/zihuan_sdk.mjs";

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [{
  type_id: "tavily_web_search", display_name: "网页搜索", category: "工具", description: "使用 Web Search Engine 搜索网页，或对单个 URL 抽取正文内容",
  input_ports: [port("web_search_engine_ref", "WebSearchEngineRef"), port("query", "String", { required: false }), port("url", "String", { required: false }), port("search_count", "Integer", { required: false })], output_ports: [port("results", { Vec: "String" })],
  execute: async ({ inputs, sdk }) => sdk.search.web(sdk.resources.webSearch(inputs.web_search_engine_ref), {
    query: inputs.query,
    url: inputs.url,
    search_count: inputs.search_count,
  }),
}];
