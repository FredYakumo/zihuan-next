import { port } from "../graph_engine/zihuan_sdk.mjs";

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "session_state_get", display_name: "读取会话状态", category: "消息存储", description: "读取 sender_id 当前是否处于会话中以及附加状态",
    input_ports: [port("session_ref", "SessionStateRef"), port("sender_id", "String")], output_ports: [port("in_session", "Boolean"), port("state_json", "Json")],
    execute: async ({ inputs, sdk }) => sdk.session.get(inputs.session_ref, inputs.sender_id),
  },
  {
    type_id: "session_state_clear", display_name: "清除会话状态", category: "消息存储", description: "清除 sender_id 当前会话状态",
    input_ports: [port("session_ref", "SessionStateRef"), port("sender_id", "String")], output_ports: [port("cleared", "Boolean")],
    execute: async ({ inputs, sdk }) => ({ cleared: await sdk.session.clear(inputs.session_ref, inputs.sender_id) }),
  },
  {
    type_id: "session_state_try_claim", display_name: "尝试占用会话", category: "消息存储", description: "原子检查并占用 sender_id 会话状态",
    input_ports: [port("session_ref", "SessionStateRef"), port("sender_id", "String"), port("state_json", "Json", { required: false })], output_ports: [port("claimed", "Boolean"), port("in_session", "Boolean"), port("state_json", "Json")],
    execute: async ({ inputs, sdk }) => sdk.session.tryClaim(inputs.session_ref, inputs.sender_id, inputs.state_json),
  },
  {
    type_id: "session_state_release", display_name: "释放会话占用", category: "消息存储", description: "释放 sender_id 当前持有的会话占用",
    input_ports: [port("session_ref", "SessionStateRef"), port("sender_id", "String")], output_ports: [port("released", "Boolean")],
    execute: async ({ inputs, sdk }) => ({ released: await sdk.session.release(inputs.session_ref, inputs.sender_id) }),
  },
];
