import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [{
  type_id: "llm_infer", display_name: "LLM推理", category: "AI", description: "使用LLModel引用对消息列表进行一次推理",
  input_ports: [port("llm_model", "LLModel"), port("messages", { Vec: "LLMMessage" })], output_ports: [port("response", { Vec: "LLMMessage" })],
  execute: async ({ inputs, zihuan }) => zihuan.models.infer(inputs.llm_model, inputs.messages),
}];
