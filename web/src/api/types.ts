// TypeScript types mirroring the Rust API structs

export interface PortInfo {
  name: string;
  data_type: string;
  description: string | null;
  required: boolean;
}

export interface NodeTypeInfo {
  type_id: string;
  display_name: string;
  category: string;
  description: string;
  input_ports: PortInfo[];
  output_ports: PortInfo[];
  has_dynamic_input_ports: boolean;
  has_dynamic_output_ports: boolean;
  is_event_producer: boolean;
}

export interface RegistryResponse {
  types: NodeTypeInfo[];
  categories: string[];
}

export interface GraphPosition {
  x: number;
  y: number;
}

export interface GraphSize {
  width: number;
  height: number;
}

export interface Port {
  name: string;
  data_type: string | object;
  description: string | null;
  required: boolean;
}

export interface PortBinding {
  kind: "Hyperparameter" | "Variable";
  name: string;
}

export interface NodeDefinition {
  id: string;
  name: string;
  description: string | null;
  node_type: string;
  input_ports: Port[];
  output_ports: Port[];
  dynamic_input_ports: boolean;
  dynamic_output_ports: boolean;
  position: GraphPosition | null;
  size: GraphSize | null;
  inline_values: Record<string, unknown>;
  port_bindings: Record<string, PortBinding>;
  has_error: boolean;
  has_cycle: boolean;
}

export interface EdgeDefinition {
  from_node_id: string;
  from_port: string;
  to_node_id: string;
  to_port: string;
}

export interface HyperParameter {
  name: string;
  data_type: string;
  group: string;
  required: boolean;
  description: string | null;
}

export interface GraphVariable {
  name: string;
  data_type: string;
  initial_value: unknown | null;
}

export interface NodeGraphDefinition {
  nodes: NodeDefinition[];
  edges: EdgeDefinition[];
  hyperparameter_groups: string[];
  hyperparameters: HyperParameter[];
  variables: GraphVariable[];
}

export interface GraphTabInfo {
  id: string;
  name: string;
  file_path: string | null;
  dirty: boolean;
  node_count: number;
  edge_count: number;
}

export interface ValidationIssue {
  severity: "error" | "warning";
  message: string;
}

export interface ValidationResult {
  issues: ValidationIssue[];
  cycle_nodes: string[];
  has_errors: boolean;
}

export interface TaskEntry {
  id: string;
  graph_name: string;
  graph_session_id: string;
  start_time: string;
  is_running: boolean;
  end_time: string | null;
}

// WebSocket message types
export type ServerMessage =
  | { type: "TaskStarted"; task_id: string; graph_name: string; graph_session_id: string }
  | { type: "TaskFinished"; task_id: string; success: boolean }
  | { type: "TaskStopped"; task_id: string }
  | { type: "LogMessage"; level: string; message: string; timestamp: string }
  | { type: "GraphValidationResult"; graph_id: string; issues: ValidationIssue[] };

export type ClientMessage =
  | { type: "Subscribe"; graph_id: string }
  | { type: "Unsubscribe"; graph_id: string }
  | { type: "Ping" };
