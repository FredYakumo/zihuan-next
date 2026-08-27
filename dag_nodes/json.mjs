import { port } from "../graph_engine/zihuan_sdk.mjs";

const baseInputs = () => [
  port("json", "Json", { description: "待提取字段的 JSON 对象" }),
  port("fields_config", "Json", { required: false, description: "提取字段配置，由字段编辑器维护" }),
];

const fields = (value) => {
  if (value === null || value === undefined) return [];
  if (!Array.isArray(value)) throw new Error("Invalid fields_config: expected an array");
  const names = new Set();
  return value.map((field) => {
    const name = String(field?.name ?? "").trim();
    if (!name) throw new Error("提取字段名不能为空");
    if (names.has(name)) throw new Error(`提取字段名重复：${name}`);
    names.add(name);
    return { name, data_type: field.data_type ?? "Any" };
  });
};

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [{
  type_id: "json_extract", display_name: "提取 JSON 字段", category: "工具", description: "通过字段编辑器配置要提取的字段列表，并动态输出对应类型的字段值",
  dynamic_output_ports: true,
  input_ports: baseInputs(), output_ports: [],
  resolve_ports: ({ inline_values }) => ({
    input_ports: baseInputs(),
    output_ports: fields(inline_values.fields_config).map((field) => port(field.name, field.data_type, { description: `从输入 JSON 中提取字段 '${field.name}'` })),
  }),
  execute: ({ inputs, inline_values }) => {
    const configured = fields(inputs.fields_config ?? inline_values.fields_config);
    if (!inputs.json || Array.isArray(inputs.json) || typeof inputs.json !== "object") throw new Error("json_extract 节点要求输入 JSON 必须为对象");
    const outputs = {};
    for (const field of configured) {
      if (!Object.hasOwn(inputs.json, field.name)) throw new Error(`JSON 中不存在字段 '${field.name}'`);
      outputs[field.name] = inputs.json[field.name];
    }
    return outputs;
  },
}];
