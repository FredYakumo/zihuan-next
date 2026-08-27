import { port } from "../graph_engine/zihuan_sdk.mjs";

const variables = (template) => {
  const result = [];
  const seen = new Set();
  for (const match of String(template ?? "").matchAll(/\$\{([^}]*)\}/g)) {
    const name = match[1].trim();
    if (name && !seen.has(name)) {
      seen.add(name);
      result.push(name);
    }
  }
  return result;
};

const display = (value) => {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
};

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [{
  type_id: "format_string", display_name: "格式化字符串", category: "工具", description: "通过 ${变量名} 模板语法将输入变量格式化为字符串",
  dynamic_input_ports: true,
  input_ports: [port("template", "String", { required: false, hidden: true })],
  output_ports: [port("output", "String", { description: "格式化后的字符串" })],
  resolve_ports: ({ inline_values }) => ({
    input_ports: [
      port("template", "String", { required: false, hidden: true }),
      ...variables(inline_values.template).map((name) => port(name, "Any", { description: `变量 ${name}` })),
    ],
  }),
  execute: ({ inputs, inline_values }) => {
    const template = String(inline_values.template ?? inputs.template ?? "");
    const output = template.replace(/\$\{([^}]*)\}/g, (full, rawName) => {
      const name = rawName.trim();
      return Object.hasOwn(inputs, name) ? display(inputs[name]) : full;
    });
    return { output };
  },
}];
