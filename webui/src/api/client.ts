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

const BASE = "/api";

async function request<T>(
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
  logs(taskId: string): Promise<{ entries: TaskLogEntry[] }> {
    return request("GET", `/tasks/${taskId}/logs`);
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
};
