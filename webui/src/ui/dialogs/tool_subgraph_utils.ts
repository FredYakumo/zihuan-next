import type { DataTypeMetaData, EdgeDefinition, NodeDefinition, NodeGraphDefinition, Port } from "../../api/types";
import type { BrainToolDefinition, FunctionPortDef } from "./types";

const FUNCTION_INPUTS_NODE_ID = "__function_inputs__";
const FUNCTION_OUTPUTS_NODE_ID = "__function_outputs__";
const FUNCTION_INPUTS_NODE_TYPE = "function_inputs";
const FUNCTION_OUTPUTS_NODE_TYPE = "function_outputs";
const BRAIN_TOOL_FIXED_CONTENT_INPUT = "content";
const QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT = "message_event";
const QQ_AGENT_TOOL_OWNER_TYPE = "qq_message_agent";
const QQ_AGENT_TOOL_OUTPUT_NAME = "result";

function cloneDataType(dataType: DataTypeMetaData): DataTypeMetaData {
  return JSON.parse(JSON.stringify(dataType)) as DataTypeMetaData;
}

function clonePortDef(port: FunctionPortDef): FunctionPortDef {
  return {
    name: port.name,
    data_type: cloneDataType(port.data_type),
  };
}

function cloneGraph(graph: NodeGraphDefinition): NodeGraphDefinition {
  return JSON.parse(JSON.stringify(graph)) as NodeGraphDefinition;
}

function buildPort(
  name: string,
  dataType: DataTypeMetaData,
  description: string | null,
  options?: { hidden?: boolean },
): Port {
  return {
    name,
    data_type: cloneDataType(dataType),
    description,
    required: false,
    hidden: options?.hidden ?? false,
  };
}

function defaultGraphMetadata() {
  return {
    name: null,
    description: null,
    version: null,
  };
}

function getToolOutputs(ownerNodeType: string, tool: BrainToolDefinition): FunctionPortDef[] {
  if (ownerNodeType === QQ_AGENT_TOOL_OWNER_TYPE) {
    return [{ name: QQ_AGENT_TOOL_OUTPUT_NAME, data_type: "String" }];
  }
  return tool.outputs.map(clonePortDef);
}

export function getToolInputSignature(
  ownerNodeType: string,
  sharedInputs: FunctionPortDef[],
  tool: BrainToolDefinition,
): FunctionPortDef[] {
  return [
    ...sharedInputs.map(clonePortDef),
    ownerNodeType === QQ_AGENT_TOOL_OWNER_TYPE
      ? { name: QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT, data_type: "MessageEvent" }
      : { name: BRAIN_TOOL_FIXED_CONTENT_INPUT, data_type: "String" },
    ...tool.parameters.map((param) => ({
      name: param.name,
      data_type: cloneDataType(param.data_type),
    })),
  ];
}

function getBoundaryDefaults(graph: NodeGraphDefinition): {
  inputPosition: { x: number; y: number };
  outputPosition: { x: number; y: number };
} {
  const innerNodes = graph.nodes.filter(
    (node) => node.id !== FUNCTION_INPUTS_NODE_ID && node.id !== FUNCTION_OUTPUTS_NODE_ID,
  );
  if (innerNodes.length === 0) {
    return {
      inputPosition: { x: 80, y: 220 },
      outputPosition: { x: 820, y: 220 },
    };
  }

  const xs = innerNodes.map((node) => node.position?.x ?? 0);
  const ys = innerNodes.map((node) => node.position?.y ?? 0);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const avgY = ys.reduce((sum, value) => sum + value, 0) / ys.length;

  return {
    inputPosition: { x: minX - 280, y: avgY },
    outputPosition: { x: maxX + 280, y: avgY },
  };
}

function buildFunctionInputsNode(
  signature: FunctionPortDef[],
  graph: NodeGraphDefinition,
  existing?: NodeDefinition,
): NodeDefinition {
  const defaults = getBoundaryDefaults(graph);
  return {
    id: FUNCTION_INPUTS_NODE_ID,
    name: existing?.name ?? "函数输入",
    description: existing?.description ?? "函数子图的输入边界节点",
    node_type: FUNCTION_INPUTS_NODE_TYPE,
    input_ports: [
      buildPort("signature", "Json", "隐藏的函数签名 JSON", { hidden: true }),
      buildPort("runtime_values", "Json", "运行时注入的函数输入 JSON", { hidden: true }),
    ],
    output_ports: signature.map((port) =>
      buildPort(port.name, port.data_type, `函数输入 '${port.name}'`),
    ),
    dynamic_input_ports: false,
    dynamic_output_ports: true,
    position: existing?.position ?? defaults.inputPosition,
    size: existing?.size ?? { width: 220, height: 120 },
    inline_values: {
      ...(existing?.inline_values ?? {}),
      signature: signature.map(clonePortDef),
    },
    port_bindings: existing?.port_bindings ?? {},
    has_error: existing?.has_error ?? false,
    has_cycle: existing?.has_cycle ?? false,
    disabled: existing?.disabled ?? false,
  };
}

function buildFunctionOutputsNode(
  signature: FunctionPortDef[],
  graph: NodeGraphDefinition,
  existing?: NodeDefinition,
): NodeDefinition {
  const defaults = getBoundaryDefaults(graph);
  return {
    id: FUNCTION_OUTPUTS_NODE_ID,
    name: existing?.name ?? "函数输出",
    description: existing?.description ?? "函数子图的输出边界节点",
    node_type: FUNCTION_OUTPUTS_NODE_TYPE,
    input_ports: [
      buildPort("signature", "Json", "隐藏的函数签名 JSON", { hidden: true }),
      ...signature.map((port) => buildPort(port.name, port.data_type, `函数输出 '${port.name}'`)),
    ],
    output_ports: [],
    dynamic_input_ports: true,
    dynamic_output_ports: false,
    position: existing?.position ?? defaults.outputPosition,
    size: existing?.size ?? { width: 220, height: 120 },
    inline_values: {
      ...(existing?.inline_values ?? {}),
      signature: signature.map(clonePortDef),
    },
    port_bindings: existing?.port_bindings ?? {},
    has_error: existing?.has_error ?? false,
    has_cycle: existing?.has_cycle ?? false,
    disabled: existing?.disabled ?? false,
  };
}

function pruneInvalidEdges(graph: NodeGraphDefinition): EdgeDefinition[] {
  const validInputs = new Map<string, Set<string>>();
  const validOutputs = new Map<string, Set<string>>();

  for (const node of graph.nodes) {
    validInputs.set(node.id, new Set(node.input_ports.map((port) => port.name)));
    validOutputs.set(node.id, new Set(node.output_ports.map((port) => port.name)));
  }

  return graph.edges.filter((edge) =>
    validOutputs.get(edge.from_node_id)?.has(edge.from_port) &&
    validInputs.get(edge.to_node_id)?.has(edge.to_port),
  );
}

export function ensureToolSubgraphSignature(
  ownerNodeType: string,
  sharedInputs: FunctionPortDef[],
  tool: BrainToolDefinition,
): BrainToolDefinition {
  const subgraph = cloneGraph(tool.subgraph ?? {
    nodes: [],
    edges: [],
    hyperparameter_groups: [],
    hyperparameters: [],
    variables: [],
    metadata: defaultGraphMetadata(),
  });
  subgraph.hyperparameter_groups ??= [];
  subgraph.hyperparameters ??= [];
  subgraph.variables ??= [];
  subgraph.metadata ??= defaultGraphMetadata();

  const inputSignature = getToolInputSignature(ownerNodeType, sharedInputs, tool);
  const outputSignature = getToolOutputs(ownerNodeType, tool);
  const existingInputs = subgraph.nodes.find((node) => node.id === FUNCTION_INPUTS_NODE_ID);
  const existingOutputs = subgraph.nodes.find((node) => node.id === FUNCTION_OUTPUTS_NODE_ID);
  const innerNodes = subgraph.nodes.filter(
    (node) => node.id !== FUNCTION_INPUTS_NODE_ID && node.id !== FUNCTION_OUTPUTS_NODE_ID,
  );

  subgraph.nodes = [
    buildFunctionInputsNode(inputSignature, subgraph, existingInputs),
    buildFunctionOutputsNode(outputSignature, subgraph, existingOutputs),
    ...innerNodes,
  ];
  subgraph.edges = pruneInvalidEdges(subgraph);

  return {
    ...tool,
    outputs: outputSignature,
    subgraph,
  };
}
