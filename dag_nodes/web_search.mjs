import { port } from "#zihuan-sdk";

const connectionField = { key: "config_id", data_type: "String", description: "选择系统中的 Web Search Engine 连接配置", required: true, widget: "connection_select", connection_kind: "web_search_engine" };

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
    {
        type_id: "tavily_provider", display_name: "Web Search Engine Provider", category: "AI", description: "从系统连接中选择 Web Search Engine 配置，输出 WebSearchEngineRef 引用",
        config_fields: [connectionField], input_ports: [], output_ports: [port("tavily_ref", "WebSearchEngineRef")],
        execute: async ({ inline_values, zihuan }) => ({ tavily_ref: await zihuan.search.createProvider(inline_values.config_id ?? inline_values.connection_id) }),
    },
    {
        type_id: "tavily_search", display_name: "网页搜索", category: "AI", description: "使用 WebSearchEngineRef 执行网页搜索并输出包含标题、链接和内容的 Vec<String>",
        input_ports: [port("tavily_ref", "WebSearchEngineRef"), port("query", "String"), port("search_count", "Integer")], output_ports: [port("results", { Vec: "String" })],
        execute: async ({ inputs, zihuan }) => zihuan.search.query(inputs.tavily_ref, inputs.query, inputs.search_count),
    },
    {
        type_id: "tavily_web_search", display_name: "网页搜索", category: "工具", description: "使用 Web Search Engine 搜索网页，或对单个 URL 抽取正文内容",
        input_ports: [port("web_search_engine_ref", "WebSearchEngineRef"), port("query", "String", { required: false }), port("url", "String", { required: false }), port("search_count", "Integer", { required: false })], output_ports: [port("results", { Vec: "String" })],
        execute: async ({ inputs, zihuan }) => zihuan.search.web(zihuan.resources.webSearch(inputs.web_search_engine_ref), {
            query: inputs.query,
            url: inputs.url,
            search_count: inputs.search_count,
        }),
    }

];
