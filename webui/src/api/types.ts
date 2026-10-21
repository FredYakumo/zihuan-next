// TypeScript types mirroring the Rust API structs
export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

/**
 * Object-variant form of a DataType as serialized by Rust serde.
 * Tuple variants like `Vec(T)` become `{"Vec": T}`, `Custom(s)` becomes `{"Custom": s}`.
 */
export type DataTypeMetaDataObject = { Vec: DataTypeMetaData } | { Custom: string };

/**
 * Raw DataType as received from the backend — either a plain string (unit variant)
 * or a JSON object (tuple variant). Mirrors the Rust `DataType` enum.
 *
 * Unit variants:  "Any" | "String" | "Integer" | "Float" | "Boolean" | "Json" | "Binary" |
 *                 "Password" | "MessageEvent" | "Sender" | "LLMMessage" | "QQMessage" | "FunctionTools" |
 *                 "BotAdapterRef" | "S3Ref" | "RedisRef" | "MySqlRef" | "WebSearchEngineRef" | "SqliteRef" |
 *                 "SessionStateRef" | "LLMMessageSessionCacheRef" | "LLModel" | "LoopControlRef"
 * Tuple variants: { Vec: DataTypeMetaData } | { Custom: string }
 */
export type DataTypeMetaData = string | DataTypeMetaDataObject;
export interface UiComponentDefinition { type: string; props?: Record<string, JsonValue>; value_key?: string; event?: string; children?: UiComponentDefinition[]; }
export interface NodeUiDefinition { template_path?: string; style_path?: string; card?: UiComponentDefinition[]; panel?: UiComponentDefinition[]; }

export interface PortInfo {
  name: string;
  data_type: string;
  description: string | null;
  required: boolean;
  hidden: boolean;
}

export interface NodeConfigFieldInfo {
  key: string;
  data_type: string;
  description: string | null;
  required: boolean;
  widget: string;
  connection_kind: string | null;
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
  config_fields: NodeConfigFieldInfo[];
  ui_template?: string | null;
  ui_template_error?: string | null;
  ui?: NodeUiDefinition | null;
}

export interface RegistryResponse {
  types: NodeTypeInfo[];
  categories: string[];
}

/** One registered node type that an agent or sub-agent may name as a tool. */
export interface NodeToolInfo {
  id: string;
  name: string;
  category: string;
  description: string;
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
  data_type: DataTypeMetaData;
  description: string | null;
  required: boolean;
  hidden?: boolean;
}

export interface PortBinding {
  kind: "hyperparameter" | "variable";
  name: string;
}

export interface NodeDefinition {
  id: string;
  name: string;
  description: string | null;
  node_type: string;
  input_ports: Port[];
  output_ports: Port[];
  output?: unknown | null;
  execution_time?: string | null;
  dynamic_input_ports: boolean;
  dynamic_output_ports: boolean;
  position: GraphPosition | null;
  size: GraphSize | null;
  inline_values: Record<string, unknown>;
  ui_state?: unknown | null;
  port_bindings: Record<string, PortBinding>;
  has_error: boolean;
  has_cycle: boolean;
  disabled?: boolean;
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
  data_type: DataTypeMetaData;
  initial_value: unknown | null;
}

export interface GraphMetadata {
  name: string | null;
  description: string | null;
  version: string | null;
}

export interface GraphPortDef {
  name: string;
  data_type: DataTypeMetaData;
  description?: string;
}

export interface NodeGraphDefinition {
  nodes: NodeDefinition[];
  edges: EdgeDefinition[];
  graph_inputs: GraphPortDef[];
  graph_outputs: GraphPortDef[];
  hyperparameter_groups: string[];
  hyperparameters: HyperParameter[];
  variables: GraphVariable[];
  metadata: GraphMetadata;
  accepts_agent_events?: boolean;
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
  task_type: "node_graph" | "agent_service" | "workspace_chat" | "scheduled_job";
  graph_name: string;
  graph_session_id: string;
  chat_session_id: string | null;
  file_path: string | null;
  is_workflow_set: boolean;
  start_time: string;
  is_running: boolean;
  end_time: string | null;
  duration_ms: number | null;
  user_ip: string | null;
  status: "running" | "success" | "failed" | "stopped" | "waiting_auth";
  error_message: string | null;
  result_summary: string | null;
  log_path: string | null;
  can_rerun: boolean;
}

export interface TaskLogEntry {
  timestamp: string;
  level: string;
  message: string;
}


export interface SchedulerJobStatus {
  task_name: string;
  script: string;
  entry: string;
  description: string;
  language: string;
  builtin: boolean;
}


export interface SchedulerServiceStatus {
  source_service: string;
  agent_id: string;
}

/** Read-only snapshot of the scheduler registry. */
export interface SchedulerStatus {
  jobs: SchedulerJobStatus[];
  services: SchedulerServiceStatus[];
}

/**
 * One planned execution of a job at a fixed time. The record is written when the task is
 * scheduled, updated with the outcome once it runs, and kept afterwards as history.
 */
export interface ScheduledTaskEntry {
  id: string;
  task_name: string;
  source_service: string;
  triggered_by: string | null;
  start_time: string;
  end_time: string | null;
  status: "pending" | "running" | "succeeded" | "failed" | "cancelled";
  related_task_ids: string[];
  info_summary: string | null;
}

// WebSocket message types

export type ServerMessage =
  | { type: "TaskStarted"; task_id: string; graph_name: string; graph_session_id: string }
  | { type: "TaskFinished"; task_id: string; success: boolean; error?: string }
  | { type: "TaskStopped"; task_id: string }
  | { type: "LogMessage"; level: string; message: string; timestamp: string }
  | { type: "GraphValidationResult"; graph_id: string; issues: ValidationIssue[] }
  | { type: "NodeUiUpdate"; task_id: string; graph_session_id: string; node_id: string; revision: number; state: JsonValue }
  | { type: "QqContext"; event: QqChatContextEvent };

export interface TokenUsage {
  prompt_tokens?: number;
  cached_prompt_tokens?: number;
  prompt_cache_miss_tokens?: number;
  completion_tokens?: number;
  total_tokens?: number;
}

export interface QqContextMediaRef {
  media_id: string;
  rustfs_path?: string;
  original_source?: string;
  mime_type?: string;
  name?: string;
  size_bytes?: number;
}

export interface QqContextQuoted {
  message_id: string;
  sender_name?: string;
  sender_id?: string;
  text: string;
}

/** One step of a sub-agent's own tool-calling loop, shown inside the sub-agent box. */
export type QqSubAgentEntry =
  | { kind: "input"; system_prompt?: string; user_prompt: string }
  | { kind: "assistant"; iteration?: number; content: string; reasoning?: string; final_response: boolean }
  /** Live delta of the streaming response; the settled `assistant` entry follows it. */
  | { kind: "assistant_chunk"; content: string; reasoning: string }
  | { kind: "tool_started"; call_id: string; name: string; arguments: unknown }
  | { kind: "tool_finished"; call_id: string; name: string; result: string; duration_ms: number };

export type QqChatContextEvent =
  | {
      kind: "turn_started";
      agent_id: string;
      sender_id: string;
      sender_name: string;
      group_id?: string;
      group_name?: string;
      is_group: boolean;
      turn_id: string;
      created_at: string;
      user_text: string;
      /** The message the user quoted, rendered as a collapsible block inside the user bubble. */
      quoted?: QqContextQuoted;
      media: QqContextMediaRef[];
      /** Steer follow-up of the previous turn: shown in the same block, without a divider. */
      continues_previous: boolean;
    }
  | { kind: "tool_started"; agent_id: string; sender_id: string; turn_id: string; call_id: string; tool_name: string; arguments: JsonValue }
  | { kind: "tool_finished"; agent_id: string; sender_id: string; turn_id: string; call_id: string; tool_name: string; result: string; duration_ms: number }
  | { kind: "stage"; agent_id: string; sender_id: string; turn_id: string; at: string; stage: string; detail: JsonValue }
  | {
      kind: "sub_agent";
      agent_id: string;
      sender_id: string;
      turn_id: string;
      at: string;
      /** Enclosing tool call ids, outermost first; the last one owns this step. */
      path: string[];
      event: QqSubAgentEntry;
    }
  | {
      kind: "turn_finished";
      agent_id: string;
      sender_id: string;
      turn_id: string;
      assistant_text?: string;
      reasoning_text?: string;
      usage?: TokenUsage;
      generation_ms?: number;
      output_tokens_per_second?: number;
    };


export type ClientMessage =
  | { type: "Subscribe"; graph_id: string }
  | { type: "Unsubscribe"; graph_id: string }
  | { type: "Ping" }
  | { type: "NodeUiEvent"; graph_session_id: string; task_id?: string; node_id: string; event: string; payload: JsonValue };
