import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "llm_message_session_cache", display_name: "LLMMessage 会话暂存", category: "消息存储", description: "根据缓存 Ref、sender_id 和消息列表向当前运行期会话历史追加 Vec<LLMMessage>",
    input_ports: [port("cache_ref", "LLMMessageSessionCacheRef"), port("sender_id", "String"), port("messages", { Vec: "LLMMessage" })], output_ports: [port("success", "Boolean")],
    execute: async ({ inputs, zihuan }) => ({ success: await zihuan.messageCache.append(inputs.cache_ref, inputs.sender_id, inputs.messages) }),
  },
  {
    type_id: "llm_message_session_cache_get", display_name: "获取 LLMMessage 历史", category: "消息存储", description: "根据 LLMMessage 会话缓存 Ref 和 sender_id 读取当前运行期累计的 Vec<LLMMessage>",
    input_ports: [port("cache_ref", "LLMMessageSessionCacheRef"), port("sender_id", "String"), port("fallback", { Vec: "LLMMessage" }, { required: false })], output_ports: [port("messages", { Vec: "LLMMessage" })],
    execute: async ({ inputs, zihuan }) => ({ messages: await zihuan.messageCache.get(inputs.cache_ref, inputs.sender_id, inputs.fallback) }),
  },
  {
    type_id: "llm_message_session_cache_set", display_name: "覆写 LLMMessage 历史", category: "消息存储", description: "根据缓存 Ref、sender_id 和消息列表覆写当前运行期累计的 Vec<LLMMessage>",
    input_ports: [port("cache_ref", "LLMMessageSessionCacheRef"), port("sender_id", "String"), port("messages", { Vec: "LLMMessage" })], output_ports: [port("success", "Boolean")],
    execute: async ({ inputs, zihuan }) => ({ success: await zihuan.messageCache.set(inputs.cache_ref, inputs.sender_id, inputs.messages) }),
  },
  {
    type_id: "llm_message_session_cache_clear", display_name: "清空 LLMMessage 历史", category: "消息存储", description: "根据缓存 Ref 和 sender_id 清空当前运行期累计的历史消息",
    input_ports: [port("cache_ref", "LLMMessageSessionCacheRef"), port("sender_id", "String")], output_ports: [port("cleared", "Boolean")],
    execute: async ({ inputs, zihuan }) => ({ cleared: await zihuan.messageCache.clear(inputs.cache_ref, inputs.sender_id) }),
  },
];
