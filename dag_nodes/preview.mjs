import { port } from "../graph_engine/zihuan_sdk.mjs";

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "preview_string", display_name: "Preview String", category: "工具", description: "在节点卡片内预览输入字符串",
    input_ports: [port("text", "String", { required: false })], output_ports: [],
    execute: () => ({}),
  },
  {
    type_id: "preview_message_list", display_name: "Preview LLMMessage List", category: "工具", description: "在节点卡片内预览 LLMMessage 列表",
    input_ports: [port("messages", { Vec: "LLMMessage" }, { required: false })], output_ports: [],
    execute: () => ({}),
  },
  {
    type_id: "qq_message_preview", display_name: "Preview QQ Messages", category: "工具", description: "在节点卡片内实时预览 QQMessage 列表（含图片）",
    input_ports: [port("messages", { Vec: "QQMessage" }, { required: false })], output_ports: [],
    execute: () => ({}),
  },
];
