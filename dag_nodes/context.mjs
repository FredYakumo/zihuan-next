import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [{
  type_id: "context_compact", display_name: "上下文压缩", category: "AI", description: "压缩 LLMMessage 历史，仅保留摘要对和最近 2 条非 tool 消息",
  input_ports: [port("llm_model", "LLModel"), port("messages", { Vec: "LLMMessage" }), port("compact_context_length", "Integer"), port("force_compact", "Boolean", { required: false })],
  output_ports: [port("messages", { Vec: "LLMMessage" }), port("did_compact", "Boolean"), port("estimated_tokens_before", "Integer"), port("estimated_tokens_after", "Integer")],
  execute: async ({ inputs, zihuan }) => zihuan.models.compactContext(inputs.llm_model, inputs.messages, inputs.compact_context_length, inputs.force_compact),
}];
