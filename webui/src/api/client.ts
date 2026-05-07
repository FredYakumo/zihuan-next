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
} from "./types";

export type { GraphTabInfo, TaskEntry, TaskLogEntry } from "./types";

const BASE = "/api";

export async function request<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error((err as { error: string }).error ?? res.statusText);
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
  ): Promise<{ id: string }> {
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
};

// Workflows
export const workflows = {
  list(): Promise<{ files: string[] }> {
    return request("GET", "/workflow_set");
  },
  listDetailed(): Promise<{ workflows: Array<{ name: string; file: string; cover_url: string | null; display_name: string | null; description: string | null; version: string | null }> }> {
    return request("GET", "/workflow_set/detailed");
  },
  save(graphId: string, name: string): Promise<{ ok: boolean; path: string }> {
    return request("POST", "/workflow_set/save", { graph_id: graphId, name });
  },
  open(file: string): Promise<{ session_id: string; migrated: boolean }> {
    return request("POST", "/file/open", { path: file });
  },
};

export interface ConnectionConfig {
  id: string;
  config_id?: string | null;
  name: string;
  enabled: boolean;
  updated_at: string;
  kind: Record<string, unknown> & { type: string };
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
  stream: boolean;
  supports_multimodal_input: boolean;
  timeout_secs: number;
  retry_count: number;
}

export interface LlmConfig {
  id: string;
  name: string;
  enabled: boolean;
  updated_at: string;
  llm: LlmServiceConfig;
}

export interface AgentToolConfig {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  tool_type: Record<string, unknown> & { type: string };
}

export interface AgentRuntimeInfo {
  agent_id: string;
  status: "stopped" | "starting" | "running" | "error";
  started_at: string | null;
  last_error: string | null;
}

export interface AgentConfig {
  id: string;
  name: string;
  enabled: boolean;
  auto_start: boolean;
  is_default: boolean;
  updated_at: string;
  agent_type: Record<string, unknown> & { type: string };
  tools: AgentToolConfig[];
}

export interface AgentWithRuntime extends AgentConfig {
  runtime: AgentRuntimeInfo;
  qq_chat_profile?: {
    bot_user_id?: string | null;
    bot_nickname?: string | null;
    bot_avatar_url?: string | null;
  } | null;
}

export interface ChatStreamEvent {
  type: "start" | "delta" | "done" | "error";
  session_id?: string;
  message_id?: string;
  index?: number;
  token?: string;
  error?: string;
}

export interface ChatToolCall {
  id: string;
  type_name: string;
  function: {
    name: string;
    arguments: unknown;
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
  timestamp: string;
  stream_index?: number | null;
  trace_id: string;
  message_id: string;
  tool_calls?: ChatToolCall[];
  tool_call_id?: string | null;
}

export interface ChatSessionSummary {
  session_id: string;
  updated_at: string;
  agent_id?: string | null;
  agent_name?: string | null;
  agent_type?: string | null;
  agent_avatar_url?: string | null;
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
    }): Promise<ConnectionConfig> {
      return request("POST", "/system/connections", payload);
    },
    update(id: string, payload: {
      name: string;
      enabled: boolean;
      kind: Record<string, unknown>;
    }): Promise<ConnectionConfig> {
      return request("PUT", `/system/connections/${id}`, payload);
    },
    delete(id: string): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/connections/${id}`);
    },
  },
  llm: {
    list(): Promise<LlmConfig[]> {
      return request("GET", "/system/llm-refs");
    },
    create(payload: {
      name: string;
      enabled: boolean;
      llm: LlmServiceConfig;
    }): Promise<LlmConfig> {
      return request("POST", "/system/llm-refs", payload);
    },
    update(id: string, payload: {
      name: string;
      enabled: boolean;
      llm: LlmServiceConfig;
    }): Promise<LlmConfig> {
      return request("PUT", `/system/llm-refs/${id}`, payload);
    },
    delete(id: string): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/llm-refs/${id}`);
    },
  },
  agents: {
    list(): Promise<AgentWithRuntime[]> {
      return request("GET", "/system/agents");
    },
    create(payload: {
      name: string;
      enabled: boolean;
      auto_start: boolean;
      is_default: boolean;
      agent_type: Record<string, unknown>;
      tools: AgentToolConfig[];
    }): Promise<AgentConfig> {
      return request("POST", "/system/agents", payload);
    },
    update(id: string, payload: {
      name: string;
      enabled: boolean;
      auto_start: boolean;
      is_default: boolean;
      agent_type: Record<string, unknown>;
      tools: AgentToolConfig[];
    }): Promise<AgentConfig> {
      return request("PUT", `/system/agents/${id}`, payload);
    },
    delete(id: string): Promise<{ ok: boolean }> {
      return request("DELETE", `/system/agents/${id}`);
    },
    start(id: string): Promise<{ ok: boolean; runtime: AgentRuntimeInfo }> {
      return request("POST", `/system/agents/${id}/start`);
    },
    stop(id: string): Promise<{ ok: boolean; runtime: AgentRuntimeInfo }> {
      return request("POST", `/system/agents/${id}/stop`);
    },
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
};

export const chat = {
  async stream(
    payload: {
      agent_id: string;
      session_id?: string | null;
      stream?: boolean;
      messages: Array<{
        role: string;
        content: string;
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
