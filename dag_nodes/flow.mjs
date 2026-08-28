import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "set_variable", display_name: "设置变量", category: "工具", description: "将输入值写入运行期节点图变量",
    dynamic_input_ports: true,
    input_ports: [port("variable_name", "String", { required: false }), port("variable_type", "String", { required: false }), port("value", "Any", { required: false })], output_ports: [],
    execute: async ({ inputs, inline_values, zihuan }) => {
      const name = inputs.variable_name ?? inline_values.variable_name;
      if (!name) throw new Error("未选择变量");
      await zihuan.variables.set(name, inputs.value);
      return {};
    },
  },
  {
    type_id: "and_then", display_name: "And Then", category: "工具", description: "等待两个输入都到齐后，原样透传第二个输入",
    input_ports: [port("first", "Any"), port("second", "Any")], output_ports: [port("output", "Any")],
    execute: ({ inputs }) => ({ output: inputs.second }),
  },
  {
    type_id: "any_of", display_name: "Any Of", category: "工具", description: "任意一个输入到齐后原样透传",
    input_ports: [port("first", "Any", { required: false }), port("second", "Any", { required: false })], output_ports: [port("output", "Any")],
    execute: ({ inputs }) => ({ output: inputs.first ?? inputs.second }),
  },
  {
    type_id: "conditional", display_name: "条件分支", category: "工具", description: "根据条件选择不同的输出分支",
    input_ports: [port("condition", "Boolean"), port("true_value", "Any"), port("false_value", "Any")], output_ports: [port("result", "Any"), port("branch_taken", "String")],
    execute: ({ inputs }) => ({ result: inputs.condition ? inputs.true_value : inputs.false_value, branch_taken: inputs.condition ? "true" : "false" }),
  },
  {
    type_id: "conditional_router", display_name: "变量分拣器", category: "工具", description: "按布尔条件选择一路输入",
    input_ports: [port("condition", "Boolean"), port("primary", "Any"), port("fallback", "Any")], output_ports: [port("result", "Any"), port("branch_taken", "String")],
    execute: ({ inputs }) => ({ result: inputs.condition ? inputs.primary : inputs.fallback, branch_taken: inputs.condition ? "primary" : "fallback" }),
  },
  {
    type_id: "switch_gate", display_name: "开关器", category: "工具", description: "enabled 为 true 时透传输入",
    input_ports: [port("enabled", "Boolean"), port("input", "Any")], output_ports: [port("output", "Any")],
    execute: ({ inputs }) => ({ output: inputs.enabled ? inputs.input : null }),
  },
  {
    type_id: "boolean_branch", display_name: "布尔分路", category: "工具", description: "根据 condition 将输入送到 true 或 false 分支",
    input_ports: [port("condition", "Boolean"), port("input", "Any")], output_ports: [port("true_output", "Any"), port("false_output", "Any"), port("branch_taken", "String")],
    execute: ({ inputs }) => inputs.condition ? { true_output: inputs.input, branch_taken: "true" } : { false_output: inputs.input, branch_taken: "false" },
  },
];
