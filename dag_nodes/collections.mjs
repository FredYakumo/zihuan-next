import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "concat_vec", display_name: "拼接两个列表", category: "工具", description: "将 vec2 拼接到 vec1 后面",
    input_ports: [port("vec1", { Vec: "Any" }), port("vec2", { Vec: "Any" })], output_ports: [port("vec", { Vec: "Any" })],
    execute: ({ inputs }) => {
      if (!Array.isArray(inputs.vec1) || !Array.isArray(inputs.vec2)) throw new Error("vec1 与 vec2 必须为 Vec 类型");
      return { vec: [...inputs.vec1, ...inputs.vec2] };
    },
  },
  {
    type_id: "push_back_vec", display_name: "列表尾部追加元素", category: "工具", description: "将单个元素追加到列表末尾",
    input_ports: [port("vec", { Vec: "Any" }), port("element", "Any")], output_ports: [port("result", { Vec: "Any" })],
    execute: ({ inputs }) => {
      if (!Array.isArray(inputs.vec)) throw new Error("vec 输入必须为 Vec 类型");
      return { result: [...inputs.vec, inputs.element] };
    },
  },
  {
    type_id: "stack", display_name: "封装元素为数组", category: "工具", description: "将单个元素封装为单元素 List",
    input_ports: [port("element", "Any")], output_ports: [port("array", { Vec: "Any" })],
    execute: ({ inputs }) => ({ array: [inputs.element] }),
  },
  {
    type_id: "array_get", display_name: "列表取元素", category: "工具", description: "从列表中按下标取元素，支持负数下标",
    input_ports: [port("array", { Vec: "Any" }), port("index", "Integer")], output_ports: [port("element", "Any")],
    execute: ({ inputs }) => ({ element: inputs.array.at(inputs.index) ?? null }),
  },
  {
    type_id: "join_string", display_name: "拼接字符串列表", category: "工具", description: "使用分隔符将 Vec<String> 拼接为单个字符串",
    input_ports: [port("strings", { Vec: "String" }), port("delimiter", "String")], output_ports: [port("result", "String")],
    execute: ({ inputs }) => ({ result: inputs.strings.join(inputs.delimiter) }),
  },
];
