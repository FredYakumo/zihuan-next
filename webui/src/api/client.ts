// REST API client for Zihuan Next backend

import type {
  GraphTabInfo,
  NodeGraphDefinition,
  NodeDefinition,
  EdgeDefinition,
  NodeTypeInfo,
  ValidationResult,
  TaskEntry,
  TaskLogEntry,
  HyperParameter,
  GraphVariable,
  GraphMetadata,
  DataTypeMetaData,
} from "./types";

export type { GraphTabInfo, TaskEntry, TaskLogEntry } from "./types";

const BASE = "/api";

export class ApiError extends Error {
  status: number;
  code?: string;
  details: Record<string, unknown>;

  constructor(message: string, status: number, details: Record<string, unknown>) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = typeof details.code === "string" ? details.code : undefined;
    this.details = details;
  }
}

export async function request<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    cache: "no-store",
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    const details = err && typeof err === "object" ? err as Record<string, unknown> : {};
    const message = typeof details.error === "string" ? details.error : res.statusText;
    throw new ApiError(message, res.status, details);
  }
  return res.json() as Promise<T>;
}

// Registry
export const registry = {
  getTypes(): Promise<{ types: NodeTypeInfo[]; categories: string[] }> {
    return request("GET", "/registry/types");
  },
  getCategories(): Promise<string[]> {
    return request("GET", "/registry/categories");
  },
};

// Graph management
export const graphs = {
  list(): Promise<GraphTabInfo[]> {
    return request("GET", "/graphs");
  },
  create(): Promise<GraphTabInfo> {
    return request("POST", "/graphs");
  },
  get(id: string): Promise<NodeGraphDefinition> {
    return request("GET", `/graphs/${id}`);
  },
  put(id: string, graph: NodeGraphDefinition): Promise<{ ok: boolean }> {
    return request("PUT", `/graphs/${id}`, graph);
  },
  delete(id: string): Promise<{ ok: boolean }> {
    return request("DELETE", `/graphs/${id}`);
  },
  addNode(
    id: string,
    nodeType: string,
    name: string | undefined,
    x: number,
    y: number
  ): Promise<NodeDefinition> {
    return request("POST", `/graphs/${id}/nodes`, { node_type: nodeType, name, x, y });
  },
  updateNode(
    graphId: string,
    nodeId: string,
    updates: {
      name?: string;
      x?: number;
      y?: number;
      width?: number;
      height?: number;
      inline_values?: Record<string, unknown>;
      port_bindings?: Record<string, { kind: string; name: string }>;
      disabled?: boolean;
    }
  ): Promise<{ ok: boolean }> {
    return request("PUT", `/graphs/${graphId}/nodes/${nodeId}`, updates);
  },
  deleteNode(graphId: string, nodeId: string): Promise<{ ok: boolean }> {
    return request("DELETE", `/graphs/${graphId}/nodes/${nodeId}`);
  },
  addEdge(
    graphId: string,
    edge: {
      source_node: string;
      source_port: string;
      target_node: string;
      target_port: string;
    }
  ): Promise<{ ok: boolean }> {
    return request("POST", `/graphs/${graphId}/edges`, edge);
  },
  deleteEdge(
    graphId: string,
    edge: {
      source_node: string;
      source_port: string;
      target_node: string;
      target_port: string;
    }
  ): Promise<{ ok: boolean }> {
    return request("DELETE", `/graphs/${graphId}/edges`, edge);
  },
  validate(graphId: string): Promise<ValidationResult> {
    return request("POST", `/graphs/${graphId}/validate`);
  },
  execute(
    graphId: string,
    hyperparameterOverrides?: Record<string, unknown>
  ): Promise<{ task_id: string }> {
    return request("POST", `/graphs/${graphId}/execute`, {
      hyperparameter_overrides: hyperparameterOverrides ?? null,
    });
  },
  saveFile(graphId: string, path?: string): Promise<{ ok: boolean; path: string }> {
    return request("POST", `/graphs/${graphId}/file/save`, { path: path ?? null });
  },
  downloadUrl(graphId: string): string {
    return `${BASE}/graphs/${graphId}/file/download`;
  },
  getHyperparameters(graphId: string): Promise<{
    hyperparameters: HyperParameter[];
    hyperparameter_groups: string[];
    values: Record<string, unknown>;
  }> {
    return request("GET", `/graphs/${graphId}/hyperparameters`);
  },
  updateHyperparameters(
    graphId: string,
    values: Record<string, unknown>
  ): Promise<{ ok: boolean }> {
    return request("PUT", `/graphs/${graphId}/hyperparameters`, { values });
  },
  getVariables(graphId: string): Promise<GraphVariable[]> {
    return request("GET", `/graphs/${graphId}/variables`);
  },
  updateVariables(
    graphId: string,
    variables: GraphVariable[]
  ): Promise<{ ok: boolean }> {
    return request("PUT", `/graphs/${graphId}/variables`, { variables });
  },
  getMetadata(graphId: string): Promise<GraphMetadata> {
    return request("GET", `/graphs/${graphId}/metadata`);
  },
  updateMetadata(
    graphId: string,
    metadata: GraphMetadata
  ): Promise<{ ok: boolean }> {
    return request("PUT", `/graphs/${graphId}/metadata`, metadata);
  },
};

// File I/O
export const fileIO = {
  open(serverPath: string): Promise<{ session_id: string; migrated: boolean }> {
    return request("POST", "/file/open", { path: serverPath });
  },
  async upload(file: File): Promise<{ session_id: string }> {
    const bytes = await file.arrayBuffer();
    const res = await fetch(`${BASE}/file/upload`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: bytes,
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error((err as { error: string }).error ?? res.statusText);
    }
    return res.json() as Promise<{ session_id: string }>;
  },
  async uploadImage(
    file: File,
  ): Promise<{ url: string; key: string; name: string }> {
    if (!file.type.startsWith("image/")) {
      throw new Error(`不支持的文件类型: ${file.type || "未知"}`);
    }
    const bytes = await file.arrayBuffer();
    const url = `${BASE}/file/upload-image?name=${encodeURIComponent(file.name)}`;
    const res = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": file.type },
      body: bytes,
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error((err as { error: string }).error ?? res.statusText);
    }
    return res.json() as Promise<{ url: string; key: string; name: string }>;
  },
  listTextEmbeddingModels(): Promise<{ models: string[] }> {
    return request("GET", "/models/text-embedding");
  },
  listLocalLlmModels(): Promise<{ models: LocalLlmModelInfo[] }> {
    return request("GET", "/models/llm");
  },
  listTokenizerModels(): Promise<{ models: string[] }> {
    return request("GET", "/models/tokenizer");
  },
};

// Tasks
export const tasks = {
  list(): Promise<TaskEntry[]> {
    return request("GET", "/tasks");
  },
  stop(taskId: string): Promise<{ ok: boolean }> {
    return request("POST", `/tasks/${taskId}/stop`);
  },
  rerun(taskId: string): Promise<{ task_id: string }> {
    return request("POST", `/tasks/${taskId}/rerun`, {});
  },
  logs(
    taskId: string,
    params?: { date?: string; limit?: number; offset?: number }
  ): Promise<{ entries: TaskLogEntry[]; total: number; offset: number; limit?: number }> {
    const qs = new URLSearchParams();
    if (params?.date) qs.set("date", params.date);
    if (params?.limit != null) qs.set("limit", String(params.limit));
    if (params?.offset != null) qs.set("offset", String(params.offset));
    const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
    return request("GET", `/tasks/${taskId}/logs${suffix}`);
  },
  clearFinished(): Promise<{ ok: boolean; cleared: number }> {
    return request("DELETE", "/tasks");
  },
  delete(taskId: string): Promise<{ ok: boolean }> {
    return request("DELETE", `/tasks/${taskId}`);
  },
  deleteBatch(taskIds: string[]): Promise<{ ok: boolean; deleted: number }> {
    return request("POST", "/tasks/delete-batch", { task_ids: taskIds });
  },
};

// Workflows
export const workflows = {
  list(): Promise<{ files: string[] }> {
    return request("GET", "/workflow_set");
  },
  listDetailed(): Promise<{ workflows: Array<WorkflowInfo> }> {
    return request("GET", "/workflow_set/detailed");
  },
  save(graphId: string, name: string): Promise<{ ok: boolean; path: string }> {
    return request("POST", "/workflow_set/save", { graph_id: graphId, name });
  },
  open(file: string): Promise<{ session_id: string; migrated: boolean }> {
    return request("POST", "/file/open", { path: file });
  },
};

export interface WorkflowPortDef {
  name: string;
  data_type: DataTypeMetaData;
  description: string;
}

export interface WorkflowInfo {
  name: string;
  file: string;
  cover_url: string | null;
  display_name: string | null;
  description: string | null;
  version: string | null;
  inputs: WorkflowPortDef[];
  outputs: WorkflowPortDef[];
}

export interface ConnectionConfig {
  config_id: string;
  name: string;
  enabled: boolean;
  updated_at: string;
  kind: Record<string, unknown> & { type: string };
}

export interface ConnectionMutationResponse extends ConnectionConfig {
  collection_created: boolean;
}

export interface ActiveBotAdapterInfo {
  connection_id: string;
  config_id: string;
  name: string;
  ws_url: string;
}

export interface RuntimeConnectionInstanceSummary {
  instance_id: string;
  config_id: string;
  name: string;
  kind: string;
  keep_alive: boolean;
  heartbeat_interval_secs: number | null;
  started_at: string;
  last_used_at: string;
  status: "running" | "idle" | "closing" | "error";
}

export interface LlmServiceConfig {
  model_name: string;
  api_endpoint: string;
  api_key?: string | null;
  api_style:
    | "candle_gguf"
    | "candle_hf"
    | "open_ai_chat_completions"
    | "open_ai_chat_completions_tencent_multimodal_compat"
    | "open_ai_responses"
    | "open_ai_responses_message_compat"
    | "open_ai_responses_image_url_object_compat";
  stream: boolean;
  supports_multimodal_input: boolean;
  include_reasoning_content: boolean;
  thinking_type?: "enabled" | "disabled" | null;
  reasoning_effort?: "low" | "medium" | "high" | "max" | null;
  timeout_secs: number;
  retry_count: number;
}

export interface LocalLlmModelInfo {
  model_name: string;
  kind: "text" | "vision_language";
  layout: "gguf" | "hf" | "unknown";
  available: boolean;
  reason?: string | null;
  weight_file?: string | null;
  supports_multimodal_input: boolean;
}

export type ModelRefSpec =
  | {
      type: "chat_llm";
      llm: LlmServiceConfig;
    }
  | {
      type: "text_embedding_local";
      model_name: string;
    };

export interface LlmConfig {
  config_id: string;
  name: string;
  enabled: boolean;
  updated_at: string;
  model: ModelRefSpec;
}

export interface ServiceToolConfig {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  run_duration?: "Short" | "Long";
  tool_type: Record<string, unknown> & { type: string };
}

export interface ServiceRuntimeInfo {
  agent_id: string;
  instance_id: string | null;
  status: "stopped" | "starting" | "running" | "error";
  started_at: string | null;
  last_error: string | null;
}

export interface ServiceConfig {
  config_id: string;
  name: string;
  enabled: boolean;
  auto_start: boolean;
  is_default: boolean;
  updated_at: string;
  agent_type: Record<string, unknown> & { type: string };
  tools: ServiceToolConfig[];
}

export interface ServiceWithRuntime extends ServiceConfig {
  runtime: ServiceRuntimeInfo;
  qq_chat_profile?: {
    bot_user_id?: string | null;
    bot_nickname?: string | null;
    bot_avatar_url?: string | null;
  } | null;
  avatar_url?: string | null;
}

export interface QqChatAgentServiceIgnoreRule {
  id: number;
  agent_id: string;
  sender_id: string | null;
  group_id: string | null;
  match_key: string;
  created_at: string;
  updated_at: string;
}

export interface NotificationCard {
  id: number;
  agent_id: string;
  sender_id: string;
  purpose: string;
  auth_key: string;
  failed_attempts: number;
  expires_at: string;
  elevated_until: string | null;
  consumed: boolean;
  created_at: string;
  updated_at: string;
}

export interface ChatStreamEvent {
  type: "start" | "delta" | "thinking_delta" | "done" | "error" | "tool_call_start" | "tool_call_result" | "ask_user";
  session_id?: string;
  message_id?: string;
  index?: number;
  token?: string;
  error?: string;
  // tool_call_start / tool_call_result
  call_id?: string;
  name?: string;
  arguments?: unknown;
  result?: string;
  question?: string;
  details?: string;
  placeholder?: string;
}

export interface ChatToolCall {
  id: string;
  type_name: string;
  function: {
    name: string;
    arguments: unknown;
  };
}

export interface ChatMessagePart {
  type: "text" | "image" | "video";
  text?: string;
  media?: {
    media_id: string;
    source: "upload" | "qq_chat" | "web_search" | "agent_save";
    original_source: string;
    rustfs_path: string;
    name?: string | null;
    description?: string | null;
    mime_type?: string | null;
  };
}

export interface ChatHistoryRecord {
  session_id: string;
  agent_id: string;
  agent_name: string;
  agent_type: string;
  agent_avatar_url: string | null;
  role: string;
  content: string;
  parts?: ChatMessagePart[];
  reasoning_content?: string | null;
  timestamp: string;
  stream_index?: number | null;
  trace_id: string;
  message_id: string;
  tool_calls?: ChatToolCall[];
  tool_call_id?: string | null;
  workspace_path?: string | null;
  pending_ask_user?: {
    question: string;
    details?: string | null;
    placeholder?: string | null;
  } | null;
}

export interface ChatSessionSummary {
  session_id: string;
  updated_at: string;
  agent_id?: string | null;
  agent_name?: string | null;
  agent_type?: string | null;
  agent_avatar_url?: string | null;
  workspace_path?: string | null;
  pending_ask_user?: {
    question: string;
    details?: string | null;
    placeholder?: string | null;
  } | null;
  title?: string | null;
}

export const system = {
  connections: {
    list(): Promise<ConnectionConfig[]> {
      return request("GET", "/system/connections");
    },
    listActiveBotAdapters(): Promise<ActiveBotAdapterInfo[]> {
      return request("GET", "/system/connections/active-bot-adapters");
    },
    listRuntimeInstances(params?: {
      page?: number;
      page_size?: number;
    }): Promise<{
      items: RuntimeConnectionInstanceSummary[];
      total: number;
      page: number;
      page_size: number;
    }> {
      const qs = new URLSearchParams();
      if (params?.page != null) qs.set("page", String(params.page));
      if (params?.page_size != null) qs.set("page_size", String(params.page_size));
      const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
      return request("GET", `/system/connections/runtime-instances${suffix}`);
    },
    closeRuntimeInstance(instanceId: string): Promise<{ ok: boolean }> {
      return request("POST", `/system/connections/runtime-instances/${instanceId}/close`);
    },
    create(payload: {
      name: string;
      enabled: boolean;
      kind: Record<string, unknown>;
      allow_create_collection?: boolean;
    }): Promise<ConnectionMutationResponse> {
      return request("POST", "/system/connections", payload);
    },
    update(configId: string, payload: {
      name: string;
      enabled: boolean;
      kind: Record<string, unknown>;
      allow_create_collection?: boolean;
    }): Promise<ConnectionMutationResponse> {
      return request("PUT", `/system/connections/${configId}`, payload);
    },
    delete(configId: string): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/connections/${configId}`);
    },
  },
  llm: {
    list(): Promise<LlmConfig[]> {
      return request("GET", "/system/llm-refs");
    },
    create(payload: {
      name: string;
      enabled: boolean;
      model: ModelRefSpec;
    }): Promise<LlmConfig> {
      return request("POST", "/system/llm-refs", payload);
    },
    update(configId: string, payload: {
      name: string;
      enabled: boolean;
      model: ModelRefSpec;
    }): Promise<LlmConfig> {
      return request("PUT", `/system/llm-refs/${configId}`, payload);
    },
    delete(configId: string): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/llm-refs/${configId}`);
    },
  },
  services: {
    list(): Promise<ServiceWithRuntime[]> {
      return request("GET", "/system/services");
    },
    create(payload: {
      name: string;
      enabled: boolean;
      auto_start: boolean;
      is_default: boolean;
      agent_type: Record<string, unknown>;
      tools: ServiceToolConfig[];
    }): Promise<ServiceConfig> {
      return request("POST", "/system/services", payload);
    },
    update(configId: string, payload: {
      name: string;
      enabled: boolean;
      auto_start: boolean;
      is_default: boolean;
      agent_type: Record<string, unknown>;
      tools: ServiceToolConfig[];
    }): Promise<ServiceConfig> {
      return request("PUT", `/system/services/${configId}`, payload);
    },
    delete(configId: string): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/services/${configId}`);
    },
    listIgnoreRules(configId: string): Promise<QqChatAgentServiceIgnoreRule[]> {
      return request("GET", `/system/services/${configId}/ignore-rules`);
    },
    createIgnoreRule(
      configId: string,
      payload: { sender_id?: string | null; group_id?: string | null }
    ): Promise<QqChatAgentServiceIgnoreRule> {
      return request("POST", `/system/services/${configId}/ignore-rules`, payload);
    },
    updateIgnoreRule(
      configId: string,
      ruleId: number,
      payload: { sender_id?: string | null; group_id?: string | null }
    ): Promise<QqChatAgentServiceIgnoreRule> {
      return request("PUT", `/system/services/${configId}/ignore-rules/${ruleId}`, payload);
    },
    deleteIgnoreRule(configId: string, ruleId: number): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/services/${configId}/ignore-rules/${ruleId}`);
    },
    listNotifications(configId: string, limit = 12): Promise<NotificationCard[]> {
      return request("GET", `/system/services/${configId}/notifications?limit=${encodeURIComponent(String(limit))}`);
    },
    deleteNotifications(configId: string): Promise<{ ok: boolean; deleted: number }> {
      return request("DELETE", `/system/services/${configId}/notifications`);
    },
    start(configId: string): Promise<{ ok: boolean; runtime: ServiceRuntimeInfo }> {
      return request("POST", `/system/services/${configId}/start`);
    },
    stop(configId: string): Promise<{ ok: boolean; runtime: ServiceRuntimeInfo }> {
      return request("POST", `/system/services/${configId}/stop`);
    },
  },
  selectDirectory(): Promise<{ path: string | null }> {
    return request("GET", "/system/select-directory");
  },
};

// Data Explorer
export interface MysqlRecord {
  message_id: string;
  sender_id: string;
  sender_name: string;
  send_time: string;
  group_id: string | null;
  group_name: string | null;
  content: string;
  at_target_list: string | null;
  media_json: string | null;
}

export interface MysqlExploreResponse {
  records: MysqlRecord[];
  total: number;
  page: number;
  page_size: number;
}

export interface RedisKeyEntry {
  key: string;
  key_type: string;
  ttl: number;
  value_preview: string | null;
}

export interface RedisExploreResponse {
  keys: RedisKeyEntry[];
  total: number;
  page: number;
  page_size: number;
  scan_cursor: number;
}

export interface RustfsObject {
  key: string;
  size: number;
  last_modified: string | null;
  url: string;
}

export interface RustfsExploreResponse {
  objects: RustfsObject[];
  prefixes: string[];
  total: number;
  page: number;
  page_size: number;
}

export interface WeaviateSearchResult {
  object_id: string | null;
  distance: number | null;
  properties: Record<string, unknown>;
}

export interface WeaviateExploreResponse {
  items: WeaviateSearchResult[];
  total: number;
  limit: number;
  class_name: string;
  collection_schema: "image_semantic" | "agent_memory";
}

export interface AgentMemoryRecord {
  object_id: string;
  title: string;
  value: string;
  expires_at: string | null;
  sender_id_list: string[];
  group_id_list: string[];
  created_at: string;
  updated_at: string;
}

export interface QqChatMessageRateLimitUsageRow {
  sender_id: string;
  sender_name: string | null;
  scope_type: string;
  scope_key: string;
  window_unit: "minute" | "hour" | "day" | string;
  window_size: number;
  used_calls: number;
  max_calls: number | null;
  unlimited: boolean;
  updated_at: string;
}

function buildQueryString(params: Record<string, unknown>): string {
  const qs = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value != null && value !== "") {
      qs.set(key, String(value));
    }
  }
  return qs.toString();
}

export const explorer = {
  queryMysql(params: {
    connection_id: string;
    message_id?: string;
    sender_id?: string;
    sender_name?: string;
    group_id?: string;
    content?: string;
    send_time_start?: string;
    send_time_end?: string;
    page?: number;
    page_size?: number;
  }): Promise<MysqlExploreResponse> {
    const qs = buildQueryString(params as Record<string, unknown>);
    return request("GET", `/explorer/mysql?${qs}`);
  },

  queryRedis(params: {
    connection_id: string;
    pattern?: string;
    scan_cursor?: number;
    page?: number;
    page_size?: number;
  }): Promise<RedisExploreResponse> {
    const qs = buildQueryString(params as Record<string, unknown>);
    return request("GET", `/explorer/redis?${qs}`);
  },

  queryRustfs(params: {
    connection_id: string;
    prefix?: string;
    search?: string;
    page?: number;
    page_size?: number;
  }): Promise<RustfsExploreResponse> {
    const qs = buildQueryString(params as Record<string, unknown>);
    return request("GET", `/explorer/rustfs?${qs}`);
  },

  queryWeaviate(params: {
    connection_id: string;
    embedding_model_ref_id?: string;
    query?: string;
    limit?: number;
  }): Promise<WeaviateExploreResponse> {
    const qs = buildQueryString(params as Record<string, unknown>);
    return request("GET", `/explorer/weaviate?${qs}`);
  },

  createAgentMemory(
    connectionId: string,
    embeddingModelRefId: string,
    payload: {
      title: string;
      value: string;
      expires_at?: string | null;
      sender_id_list?: string[];
      group_id_list?: string[];
    },
  ): Promise<AgentMemoryRecord> {
    const qs = buildQueryString({
      connection_id: connectionId,
      embedding_model_ref_id: embeddingModelRefId,
    });
    return request("POST", `/explorer/agent-memory?${qs}`, payload);
  },

  updateAgentMemory(
    connectionId: string,
    embeddingModelRefId: string,
    objectId: string,
    payload: {
      title: string;
      value: string;
      expires_at?: string | null;
      sender_id_list?: string[];
      group_id_list?: string[];
    },
  ): Promise<AgentMemoryRecord> {
    const qs = buildQueryString({
      connection_id: connectionId,
      embedding_model_ref_id: embeddingModelRefId,
    });
    return request("PUT", `/explorer/agent-memory/${encodeURIComponent(objectId)}?${qs}`, payload);
  },

  getAgentMemory(connectionId: string, objectId: string): Promise<AgentMemoryRecord> {
    const qs = buildQueryString({ connection_id: connectionId });
    return request("GET", `/explorer/agent-memory/${encodeURIComponent(objectId)}?${qs}`);
  },

  deleteAgentMemory(connectionId: string, objectId: string): Promise<{ ok: boolean }> {
    const qs = buildQueryString({ connection_id: connectionId });
    return request("DELETE", `/explorer/agent-memory/${encodeURIComponent(objectId)}?${qs}`);
  },

  queryQqChatRateLimitUsage(agentId: string): Promise<{ items: QqChatMessageRateLimitUsageRow[] }> {
    const qs = buildQueryString({ agent_id: agentId });
    return request("GET", `/explorer/qq-chat-rate-limit-usage?${qs}`);
  },

  resetQqChatRateLimitUsage(agentId: string, senderId: string): Promise<{ ok: boolean; deleted: number }> {
    return request("POST", "/explorer/qq-chat-rate-limit-usage/reset", {
      agent_id: agentId,
      sender_id: senderId,
    });
  },
};

export const chat = {
  async stream(
    payload: {
      agent_id: string;
      session_id?: string | null;
      stream?: boolean;
      model_config_id?: string | null;
      thinking_type?: "enabled" | "disabled" | null;
      reasoning_effort?: "low" | "medium" | "high" | "max" | null;
      workspace_path?: string | null;
      messages: Array<{
        role: string;
        content: string;
        parts?: ChatMessagePart[];
        tool_calls?: ChatToolCall[];
        tool_call_id?: string | null;
      }>;
    },
    onEvent: (event: ChatStreamEvent) => void,
  ): Promise<void> {
    const res = await fetch(`${BASE}/chat/stream`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error((err as { error: string }).error ?? res.statusText);
    }
    if (!res.body) {
      throw new Error("聊天流式响应为空");
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    while (true) {
      const { value, done } = await reader.read();
      if (done) {
        break;
      }

      buffer += decoder.decode(value, { stream: true });
      while (true) {
        const splitAt = buffer.indexOf("\n\n");
        if (splitAt < 0) {
          break;
        }

        const frame = buffer.slice(0, splitAt);
        buffer = buffer.slice(splitAt + 2);
        const dataLine = frame
          .split("\n")
          .map((line) => line.trim())
          .find((line) => line.startsWith("data:"));
        if (!dataLine) {
          continue;
        }

        const data = dataLine.slice(5).trim();
        if (!data || data === "[DONE]") {
          continue;
        }

        try {
          const event = JSON.parse(data) as ChatStreamEvent;
          onEvent(event);
          if (event.type === "delta") {
            // Yield to the event loop so Vue can flush its reactive updates
            // and the browser can repaint between tokens.
            await new Promise<void>((r) => setTimeout(r, 0));
          }
        } catch (error) {
          console.warn("Failed to parse chat stream event", error, data);
        }
      }
    }
  },

  listSessions(agentId?: string): Promise<{ sessions: ChatSessionSummary[] }> {
    const qs = agentId ? `?agent_id=${encodeURIComponent(agentId)}` : "";
    return request("GET", `/chat/sessions${qs}`);
  },

  getSessionMessages(sessionId: string): Promise<{ messages: ChatHistoryRecord[] }> {
    return request("GET", `/chat/sessions/${sessionId}/messages`);
  },

  deleteSession(sessionId: string): Promise<{ ok: boolean }> {
    return request("DELETE", `/chat/sessions/${sessionId}`);
  },
};

// Setup Wizard
export interface SetupWizardState {
  completed: boolean;
  skipped: boolean;
  completed_at: string | null;
  mode: string | null;
  last_step: string | null;
  last_error: string | null;
}

export interface EnvironmentInfo {
  os: string;
  os_detail: string;
  docker_available: boolean;
  docker_compose_available: boolean;
  binary_install_available: boolean;
  binary_install_reason?: string | null;
  wsl_available?: boolean | null;
  wsl_docker_available?: boolean | null;
  cuda_version: string | null;
  compiler_version: string | null;
  proxy: string | null;
  services: Array<{
    service: string;
    detected: boolean;
    connection_test_result: string | null;
  }>;
}

export interface SetupProgressEvent {
  step: string;
  status: string;
  message: string;
  progress_percent: number | null;
  error: string | null;
}

export interface LlmSetupConfig {
  mode: string;
  model_name: string;
  model_id?: string | null;
  api_endpoint: string;
  api_key?: string | null;
  api_style: string;
}

export type ImsPlatform = "qq_napcat" | "wechat" | "telegram";

export interface ImsBotAdapterSetupConfig {
  platform: ImsPlatform;
  ws_url: string;
  qq_id?: string | null;
  token?: string | null;
}

export type DetailedSetupInstallMethod = "docker" | "binary";
export type DetailedSetupSource = "install" | "existing";
export type DetailedRelationalType = "mysql" | "sqlite";
export type DetailedSearchType = "weaviate" | "elasticsearch";

export interface DetailedDeploymentConfig {
  image: string;
  port: number;
  data_dir: string;
  container_name: string;
  restart_policy: string;
}

export interface DetailedRelationalSetupConfig {
  enabled: boolean;
  source: DetailedSetupSource;
  type: DetailedRelationalType;
  deployment: DetailedDeploymentConfig;
  host: string;
  username: string;
  password: string;
  database: string;
  sqlite_path: string;
  max_connections: number;
  acquire_timeout_secs: number;
}

export interface DetailedRustfsSetupConfig {
  enabled: boolean;
  source: DetailedSetupSource;
  deployment: DetailedDeploymentConfig;
  endpoint: string;
  bucket: string;
  region: string;
  access_key: string;
  secret_key: string;
  public_base_url: string | null;
  path_style: boolean;
}

export interface DetailedSearchSetupConfig {
  enabled: boolean;
  source: DetailedSetupSource;
  type: DetailedSearchType;
  deployment: DetailedDeploymentConfig;
  base_url: string;
  username: string | null;
  password: string | null;
  api_key: string | null;
  vector_dimensions: number;
}

export interface DetailedRedisSetupConfig {
  enabled: boolean;
  source: DetailedSetupSource;
  deployment: DetailedDeploymentConfig;
  url: string;
  username: string | null;
  password: string | null;
}

export interface DetailedSetupConfig {
  install_method: DetailedSetupInstallMethod;
  target_machine_address: string;
  expose_public_access: boolean;
  use_target_machine_address: boolean;
  relational: DetailedRelationalSetupConfig;
  rustfs: DetailedRustfsSetupConfig;
  search: DetailedSearchSetupConfig;
  redis: DetailedRedisSetupConfig;
}

export interface DetailedInstallCommandResult {
  install_command: string;
  connections: ConnectionConfig[];
}

export const setup = {
  getStatus(): Promise<SetupWizardState> {
    return request("GET", "/setup/status");
  },
  getEnvironment(): Promise<EnvironmentInfo> {
    return request("GET", "/setup/environment");
  },
  execute(payload: {
    mode: "role_based" | "detailed" | "skip";
    role?: string;
    options?: { http_proxy?: string; docker_compose_path?: string };
    llm_config?: LlmSetupConfig;
    ims_bot_adapter_config?: ImsBotAdapterSetupConfig;
    detailed_config?: DetailedSetupConfig;
  }): Promise<{ accepted: boolean; task_id: string }> {
    return request("POST", "/setup", payload);
  },
  generateDetailedInstallCommand(config: DetailedSetupConfig): Promise<DetailedInstallCommandResult> {
    return request("POST", "/setup/detailed-install-command", { detailed_config: config });
  },
  skip(): Promise<{ ok: boolean }> {
    return request("POST", "/setup/skip");
  },
  reset(): Promise<{ ok: boolean }> {
    return request("POST", "/setup/reset");
  },
  streamProgress(taskId: string, onEvent: (event: SetupProgressEvent) => void): () => void {
    const es = new EventSource(`/api/setup/progress?task_id=${taskId}`);
    es.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data) as SetupProgressEvent;
        onEvent(event);
        if (event.step === "finished" || event.status === "error") {
          es.close();
        }
      } catch (err) {
        console.warn("Failed to parse setup progress event", err, e.data);
      }
    };
    es.onerror = (err) => {
      console.warn("Setup progress SSE error", err);
      es.close();
    };
    return () => es.close();
  },
};
