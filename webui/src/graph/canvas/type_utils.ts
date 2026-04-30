import type { NodeDefinition, NodeGraphDefinition } from "../../api/types";
import { portTypeString } from "../registry";

export function isCompatibleTypes(a: string, b: string): boolean {
  if (!a || !b || a === "*" || b === "*") return true;
  const lower = (s: string) => s.toLowerCase();
  if (lower(a) === "any" || lower(b) === "any") return true;
  const isVecAny = (t: string) => /^Vec<Any>$/i.test(t);
  const isVec = (t: string) => /^Vec<.+>/i.test(t);
  if ((isVecAny(a) && isVec(b)) || (isVecAny(b) && isVec(a))) return true;
  return a === b;
}

export function visibleInputPorts(ports: NodeDefinition["input_ports"]): NodeDefinition["input_ports"] {
  return ports.filter((p) => !p.hidden);
}

export function isNodeGraphDefinitionLike(value: unknown): value is NodeGraphDefinition {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const graph = value as Partial<NodeGraphDefinition>;
  return Array.isArray(graph.nodes)
    && Array.isArray(graph.edges)
    && Array.isArray(graph.hyperparameter_groups)
    && Array.isArray(graph.hyperparameters)
    && Array.isArray(graph.variables);
}

export function hasVisibleSubgraphContent(graph: NodeGraphDefinition): boolean {
  return graph.nodes.some(
    (node) => node.id !== "__function_inputs__" && node.id !== "__function_outputs__",
  );
}

export function parseWrappedAny(dt: string): { prefix: string; inner: string } | null {
  const match = dt.match(/^([^<]+)<(.+)>$/);
  if (!match) return null;
  if (!match[2].includes("Any")) return null;
  return { prefix: match[1], inner: match[2] };
}

export function resolveConcretePortType(
  graph: NodeGraphDefinition,
  nodeId: string,
  portName: string,
  isInput: boolean,
  visited = new Set<string>(),
): string {
  const key = `${nodeId}:${isInput ? "in" : "out"}:${portName}`;
  if (visited.has(key)) return "Any";
  visited.add(key);

  const nodeDef = graph.nodes.find((node) => node.id === nodeId);
  if (!nodeDef) return "Any";

  const ports = isInput ? nodeDef.input_ports : nodeDef.output_ports;
  const port = ports.find((item) => item.name === portName);
  if (!port) return "Any";

  const dt = typeof port.data_type === "string" ? port.data_type : portTypeString(port.data_type);
  if (!dt.includes("Any")) return dt;

  if (isInput) {
    const edge = graph.edges.find((item) => item.to_node_id === nodeId && item.to_port === portName);
    if (edge) {
      const upstream = resolveConcretePortType(graph, edge.from_node_id, edge.from_port, false, visited);
      const wrapped = parseWrappedAny(dt);
      if (wrapped && upstream.startsWith(`${wrapped.prefix}<`) && !upstream.includes("Any")) {
        return upstream;
      }
      if (upstream !== "Any") return upstream;
    }
    return dt;
  }

  const wrapped = parseWrappedAny(dt);
  if (wrapped) {
    for (const input of nodeDef.input_ports) {
      const inputType = typeof input.data_type === "string" ? input.data_type : portTypeString(input.data_type);
      if (!inputType.includes("Any")) continue;
      const inputWrapped = parseWrappedAny(inputType);
      if (inputWrapped && inputWrapped.prefix === wrapped.prefix) {
        const resolved = resolveConcretePortType(graph, nodeId, input.name, true, visited);
        if (!resolved.includes("Any")) return resolved;
        if (resolved.startsWith(`${wrapped.prefix}<`)) return resolved;
      }
    }
    for (const input of nodeDef.input_ports) {
      const inputType = typeof input.data_type === "string" ? input.data_type : portTypeString(input.data_type);
      if (inputType !== "Any") continue;
      const resolved = resolveConcretePortType(graph, nodeId, input.name, true, visited);
      if (resolved !== "Any" && !resolved.includes("Any")) {
        return `${wrapped.prefix}<${resolved}>`;
      }
    }
    return dt;
  }

  for (const input of nodeDef.input_ports) {
    const inputType = typeof input.data_type === "string" ? input.data_type : portTypeString(input.data_type);
    if (inputType !== "Any") continue;
    const resolved = resolveConcretePortType(graph, nodeId, input.name, true, visited);
    if (resolved !== "Any") return resolved;
  }
  return "Any";
}
