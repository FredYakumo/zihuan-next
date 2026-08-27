import { port } from "../graph_engine/zihuan_sdk.mjs";

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "current_time", display_name: "当前时间", category: "数据", description: "输出当前本地时间字符串",
    input_ports: [], output_ports: [port("time", "String")],
    execute: () => {
      const date = new Date();
      const pad = (value) => String(value).padStart(2, "0");
      return { time: `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}` };
    },
  },
  {
    type_id: "string_data", display_name: "String Data", category: "数据", description: "字符串数据源，通过UI输入框提供字符串",
    input_ports: [], output_ports: [port("text", "String")],
    execute: ({ inline_values }) => ({ text: inline_values.text ?? "" }),
  },
  {
    type_id: "message_list_data", display_name: "LLMMessage List Data", category: "数据", description: "LLMMessage 列表数据源，通过 UI 容器编辑器提供列表数据",
    input_ports: [port("messages", { Vec: "LLMMessage" }, { required: false, hidden: true })], output_ports: [port("messages", { Vec: "LLMMessage" })],
    execute: ({ inputs }) => ({ messages: Array.isArray(inputs.messages) ? inputs.messages : [] }),
  },
  {
    type_id: "qq_message_list_data", display_name: "QQMessageList Data", category: "数据", description: "QQ消息列表数据源，通过UI容器编辑器提供QQMessageList",
    input_ports: [port("messages", { Vec: "QQMessage" }, { required: false })], output_ports: [port("messages", { Vec: "QQMessage" })],
    execute: ({ inputs }) => ({ messages: Array.isArray(inputs.messages) ? inputs.messages : [] }),
  },
];
