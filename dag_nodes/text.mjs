import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "boolean_not", display_name: "布尔取反", category: "工具", description: "对输入的 Boolean 值取反",
    input_ports: [port("input", "Boolean")], output_ports: [port("result", "Boolean")],
    execute: ({ inputs }) => ({ result: !inputs.input }),
  },
  {
    type_id: "string_is_not_empty", display_name: "字符串非空判断", category: "工具", description: "判断字符串是否非空",
    input_ports: [port("input", "String"), port("trim_before_check", "Boolean", { required: false })], output_ports: [port("result", "Boolean")],
    execute: ({ inputs }) => ({ result: (inputs.trim_before_check ? inputs.input.trim() : inputs.input).length > 0 }),
  },
  {
    type_id: "json_parser", display_name: "JSON解析器", category: "工具", description: "将JSON字符串解析为结构化数据",
    input_ports: [port("json_string", "String")], output_ports: [port("parsed", "Json"), port("success", "Boolean")],
    execute: ({ inputs }) => {
      try { return { parsed: JSON.parse(inputs.json_string), success: true }; }
      catch { return { parsed: null, success: false }; }
    },
  },
];
