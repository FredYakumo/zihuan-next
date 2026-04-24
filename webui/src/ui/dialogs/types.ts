import type { NodeGraphDefinition, DataTypeMetaData } from "../../api/types";

export interface FunctionPortDef {
  name: string;
  data_type: DataTypeMetaData;
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
}

export interface BrainToolDefinition {
  id: string;
  name: string;
  description: string;
  parameters: ToolParamDef[];
  outputs: FunctionPortDef[];
  subgraph: NodeGraphDefinition;
}

export interface QQMessageItem {
  type: "text" | "at" | "reply";
  data: { text?: string; target?: string; id?: number };
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
