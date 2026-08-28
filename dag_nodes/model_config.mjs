import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [{
  type_id: "llm_api", display_name: "llm配置", category: "AI", description: "配置语言模型连接，输出LLModel引用",
  config_fields: [{ key: "llm_ref_id", data_type: "String", description: "选择系统中的聊天 LLM 配置", required: true, widget: "llm_ref_select", connection_kind: null }],
  input_ports: [], output_ports: [port("llm_model", "LLModel")],
  execute: async ({ inline_values, zihuan }) => ({ llm_model: await zihuan.models.fromRef(inline_values.llm_ref_id) }),
}];
