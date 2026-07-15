import type { NodeGraphDefinition, DataTypeMetaData } from "../../api/types";

export type BrainToolImplementation = "node_graph" | "built_in" | "python_script";
export type PythonToolMode = "uv_project" | "project_venv" | "custom_executable";

export interface PythonRuntimeConfig {
  kind: PythonToolMode;
  executable_path?: string | null;
}

export interface FunctionPortDef {
  name: string;
  data_type: DataTypeMetaData;
  description?: string;
}

export interface EmbeddedFunctionConfig {
  name: string;
  description: string;
  inputs: FunctionPortDef[];
  outputs: FunctionPortDef[];
  subgraph: NodeGraphDefinition;
}

export interface ToolParamDef {
  name: string;
  data_type: DataTypeMetaData;
  desc: string;
  required?: boolean;
}

export interface PythonScriptToolConfig {
  script_path: string;
  module_entry: string;
  python_mode?: PythonToolMode;
  python_runtime?: PythonRuntimeConfig | null;
  timeout_secs: number;
}

export interface BrainToolDefinition {
  id: string;
  name: string;
  description: string;
  implementation?: BrainToolImplementation;
  run_duration?: "Short" | "Long";
  built_in_kind?: "image_understand";
  python_config?: PythonScriptToolConfig | null;
  parameters: ToolParamDef[];
  outputs: FunctionPortDef[];
  subgraph: NodeGraphDefinition;
}

export interface QQMessageItem {
  type: "text" | "at" | "reply" | "image" | "forward";
  data: {
    text?: string;
    target?: string;
    id?: number;
    url?: string;
    path?: string;
    file?: string;
    object_url?: string;
    name?: string;
    summary?: string;
    content?: QQMessageItem[];
  };
}

export interface LLMMessageItem {
  role: "system" | "user" | "assistant" | "tool";
  content?: string | null;
  reasoning_content?: string | null;
  tool_call_id?: string | null;
}

export type ConnectPortChoice =
  | { kind: "existing"; targetNodeId: string; targetPortName: string }
  | { kind: "new_node" };

export interface PortSelectOption {
  portName: string;
  dataType: string;
  isInput: boolean;
}

export interface PortConnInfo {
  portName: string;
  dataType: string;
  description: string | null;
  required: boolean;
  connectedTo: Array<{ nodeName: string; portName: string }>;
}

export interface WorkflowEntry {
  name: string;
  file: string;
  cover_url: string | null;
  display_name: string | null;
  description: string | null;
  version: string | null;
}
