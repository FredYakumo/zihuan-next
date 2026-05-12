import type {
  AgentConfig,
  AgentToolConfig,
  AgentWithRuntime,
  ConnectionConfig,
  LlmConfig,
  ModelRefSpec,
  LlmServiceConfig,
} from "../api/client";

export type ConnectionType = "mysql" | "redis" | "weaviate" | "rustfs" | "bot_adapter" | "ims_bot_adapter" | "tavily";
export type WeaviateCollectionSchema = "message_record_semantic" | "image_semantic";
export type AgentTypeName = "qq_chat" | "http_stream";
export type ModelRefType = "chat_llm" | "text_embedding_local";
export type LlmApiStyle = "candle" | "open_ai_chat_completions" | "open_ai_responses";
export type ToolTargetType = "workflow_set" | "file_path" | "inline_graph";

export interface ConnectionFormState {
  id: string | null;
  name: string;
  enabled: boolean;
  type: ConnectionType;
  mysql_url: string;
  mysql_host: string;
  mysql_port: string;
  mysql_user: string;
  mysql_password: string;
  mysql_database: string;
  redis_url: string;
  weaviate_base_url: string;
  weaviate_class_name: string;
  weaviate_collection_schema: WeaviateCollectionSchema;
  rustfs_endpoint: string;
  rustfs_bucket: string;
  rustfs_region: string;
  rustfs_access_key: string;
  rustfs_secret_key: string;
  rustfs_public_base_url: string;
  rustfs_path_style: boolean;
  bot_server_url: string;
  adapter_server_url: string;
  bot_server_token: string;
  qq_id: string;
  tavily_api_token: string;
  tavily_timeout_secs: number;
}

export interface LlmFormState {
  id: string | null;
  name: string;
  enabled: boolean;
  model_type: ModelRefType;
  llm: LlmServiceConfig;
  local_model_name: string;
}

export interface ToolFormState {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  targetType: ToolTargetType;
  workflowName: string;
  filePath: string;
  inlineGraphJson: string;
  parametersJson: string;
  outputsJson: string;
}

export interface AgentFormState {
  id: string | null;
  name: string;
  enabled: boolean;
  auto_start: boolean;
  is_default: boolean;
  type: AgentTypeName;
  ims_bot_adapter_connection_id: string;
  rustfs_connection_id: string;
  bot_name: string;
  system_prompt: string;
  llm_ref_id: string;
  intent_llm_ref_id: string;
  math_programming_llm_ref_id: string;
  embedding_model_ref_id: string;
  tavily_connection_id: string;
  mysql_connection_id: string;
  weaviate_image_connection_id: string;
  max_message_length: number;
  compact_context_length: number;
  default_tools_enabled: Record<string, boolean>;
  http_bind: string;
  http_api_key: string;
  tools: ToolFormState[];
}

export type QqChatDefaultTool = {
  id: string;
  label: string;
  description: string;
};

export function isBotAdapterConnectionType(type: string): type is "bot_adapter" | "ims_bot_adapter" {
  return type === "bot_adapter" || type === "ims_bot_adapter";
}

export const QQ_CHAT_DEFAULT_TOOLS: QqChatDefaultTool[] = [
  { id: "web_search", label: "web_search", description: "联网搜索（Tavily）" },
  { id: "get_agent_public_info", label: "get_agent_public_info", description: "返回智能体公开信息" },
  { id: "get_function_list", label: "get_function_list", description: "获取功能列表" },
  { id: "get_recent_group_messages", label: "get_recent_group_messages", description: "只看群里最近几条消息，不适合按时段分析" },
  { id: "get_recent_user_messages", label: "get_recent_user_messages", description: "查询用户近期消息" },
  { id: "search_similar_images", label: "search_similar_images", description: "语义检索相似图片" },
];

export function defaultQqChatDefaultToolsEnabled(): Record<string, boolean> {
  return Object.fromEntries(QQ_CHAT_DEFAULT_TOOLS.map((tool) => [tool.id, true]));
}

export function defaultLlmConfig(): LlmServiceConfig {
  return {
    model_name: "",
    api_endpoint: "",
    api_key: "",
    api_style: "open_ai_chat_completions",
    stream: false,
    supports_multimodal_input: false,
    timeout_secs: 30,
    retry_count: 2,
  };
}

export function defaultConnectionForm(): ConnectionFormState {
  return {
    id: null,
    name: "",
    enabled: true,
    type: "mysql",
    mysql_url: "",
    mysql_host: "",
    mysql_port: "3306",
    mysql_user: "",
    mysql_password: "",
    mysql_database: "",
    redis_url: "",
    weaviate_base_url: "",
    weaviate_class_name: "",
    weaviate_collection_schema: "message_record_semantic",
    rustfs_endpoint: "",
    rustfs_bucket: "",
    rustfs_region: "",
    rustfs_access_key: "",
    rustfs_secret_key: "",
    rustfs_public_base_url: "",
    rustfs_path_style: true,
    bot_server_url: "",
    adapter_server_url: "",
    bot_server_token: "",
    qq_id: "",
    tavily_api_token: "",
    tavily_timeout_secs: 30,
  };
}

export function defaultLlmForm(): LlmFormState {
  return {
    id: null,
    name: "",
    enabled: true,
    model_type: "chat_llm",
    llm: defaultLlmConfig(),
    local_model_name: "",
  };
}

export function defaultToolForm(): ToolFormState {
  return {
    id: crypto.randomUUID(),
    name: "",
    description: "",
    enabled: true,
    targetType: "workflow_set",
    workflowName: "",
    filePath: "",
    inlineGraphJson: "{\n  \"nodes\": [],\n  \"edges\": [],\n  \"graph_inputs\": [],\n  \"graph_outputs\": [],\n  \"hyperparameter_groups\": [],\n  \"hyperparameters\": [],\n  \"variables\": [],\n  \"metadata\": { \"name\": null, \"description\": null, \"version\": null }\n}",
    parametersJson: "[]",
    outputsJson: "[]",
  };
}

export function defaultAgentForm(): AgentFormState {
  return {
    id: null,
    name: "",
    enabled: true,
    auto_start: false,
    is_default: false,
    type: "qq_chat",
    ims_bot_adapter_connection_id: "",
    rustfs_connection_id: "",
    bot_name: "",
    system_prompt: "",
    llm_ref_id: "",
    intent_llm_ref_id: "",
    math_programming_llm_ref_id: "",
    embedding_model_ref_id: "",
    tavily_connection_id: "",
    mysql_connection_id: "",
    weaviate_image_connection_id: "",
    max_message_length: 500,
    compact_context_length: 0,
    default_tools_enabled: defaultQqChatDefaultToolsEnabled(),
    http_bind: "127.0.0.1:18080",
    http_api_key: "",
    tools: [],
  };
}

export function connectionFormFromConfig(connection: ConnectionConfig): ConnectionFormState {
  const form = defaultConnectionForm();
  form.id = connection.config_id;
  form.name = connection.name;
  form.enabled = connection.enabled;
  form.type = isBotAdapterConnectionType(String(connection.kind.type ?? ""))
    ? "bot_adapter"
    : connection.kind.type as ConnectionType;
  switch (connection.kind.type) {
    case "mysql":
      form.mysql_url = String(connection.kind.url ?? "");
      applyMysqlUrlToForm(form, form.mysql_url);
      break;
    case "redis":
      form.redis_url = String(connection.kind.url ?? "");
      break;
    case "weaviate":
      form.weaviate_base_url = String(connection.kind.base_url ?? "");
      form.weaviate_class_name = String(connection.kind.class_name ?? "");
      form.weaviate_collection_schema = String(
        connection.kind.collection_schema ?? "message_record_semantic",
      ) as WeaviateCollectionSchema;
      break;
    case "rustfs":
      form.rustfs_endpoint = String(connection.kind.endpoint ?? "");
      form.rustfs_bucket = String(connection.kind.bucket ?? "");
      form.rustfs_region = String(connection.kind.region ?? "");
      form.rustfs_access_key = String(connection.kind.access_key ?? "");
      form.rustfs_secret_key = String(connection.kind.secret_key ?? "");
      form.rustfs_public_base_url = String(connection.kind.public_base_url ?? "");
      form.rustfs_path_style = Boolean(connection.kind.path_style ?? false);
      break;
    case "bot_adapter":
    case "ims_bot_adapter":
      form.bot_server_url = String(connection.kind.bot_server_url ?? "");
      form.adapter_server_url = String(connection.kind.adapter_server_url ?? "");
      form.bot_server_token = String(connection.kind.bot_server_token ?? "");
      form.qq_id = String(connection.kind.qq_id ?? "");
      break;
    case "tavily":
      form.tavily_api_token = String(connection.kind.api_token ?? "");
      form.tavily_timeout_secs = Number(connection.kind.timeout_secs ?? 30);
      break;
  }
  return form;
}

function applyMysqlUrlToForm(form: ConnectionFormState, rawUrl: string) {
  if (!rawUrl) {
    return;
  }
  try {
    const parsed = new URL(rawUrl);
    form.mysql_host = decodeURIComponent(parsed.hostname ?? "");
    form.mysql_port = parsed.port || "3306";
    form.mysql_user = decodeURIComponent(parsed.username ?? "");
    form.mysql_password = decodeURIComponent(parsed.password ?? "");
    form.mysql_database = decodeURIComponent(parsed.pathname.replace(/^\//, ""));
  } catch {
    // Keep the raw URL for backward compatibility if parsing fails.
  }
}

function buildMysqlUrl(form: ConnectionFormState): string {
  if (form.mysql_host || form.mysql_user || form.mysql_password || form.mysql_database) {
    const auth = form.mysql_user
      ? `${encodeURIComponent(form.mysql_user)}:${encodeURIComponent(form.mysql_password)}@`
      : "";
    const port = (form.mysql_port || "3306").trim();
    const database = encodeURIComponent(form.mysql_database.trim());
    return `mysql://${auth}${form.mysql_host.trim()}:${port}/${database}`;
  }
  return form.mysql_url.trim();
}

export function buildConnectionPayload(form: ConnectionFormState): {
  name: string;
  enabled: boolean;
  kind: Record<string, unknown>;
} {
  const payload = {
    name: form.name.trim(),
    enabled: form.enabled,
    kind: {} as Record<string, unknown>,
  };
  switch (form.type) {
    case "mysql":
      payload.kind = { type: "mysql", url: buildMysqlUrl(form) };
      break;
    case "redis":
      payload.kind = { type: "redis", url: form.redis_url.trim() };
      break;
    case "weaviate":
      payload.kind = {
        type: "weaviate",
        base_url: form.weaviate_base_url.trim(),
        class_name: form.weaviate_class_name.trim(),
        collection_schema: form.weaviate_collection_schema,
      };
      break;
    case "rustfs":
      payload.kind = {
        type: "rustfs",
        endpoint: form.rustfs_endpoint.trim(),
        bucket: form.rustfs_bucket.trim(),
        region: form.rustfs_region.trim(),
        access_key: form.rustfs_access_key.trim(),
        secret_key: form.rustfs_secret_key.trim(),
        public_base_url: form.rustfs_public_base_url.trim() || null,
        path_style: form.rustfs_path_style,
      };
      break;
    case "bot_adapter":
    case "ims_bot_adapter":
      payload.kind = {
        type: "bot_adapter",
        bot_server_url: form.bot_server_url.trim(),
        adapter_server_url: form.adapter_server_url.trim() || null,
        bot_server_token: form.bot_server_token.trim() || null,
        qq_id: form.qq_id.trim() || null,
      };
      break;
    case "tavily":
      payload.kind = {
        type: "tavily",
        api_token: form.tavily_api_token.trim(),
        timeout_secs: form.tavily_timeout_secs,
      };
      break;
  }
  return payload;
}

export function llmFormFromConfig(config: LlmConfig): LlmFormState {
  const form = defaultLlmForm();
  form.id = config.config_id;
  form.name = config.name;
  form.enabled = config.enabled;
  if (config.model.type === "text_embedding_local") {
    form.model_type = "text_embedding_local";
    form.local_model_name = config.model.model_name;
    return form;
  }
  return {
    ...form,
    model_type: "chat_llm",
    llm: {
      model_name: config.model.llm.model_name,
      api_endpoint: config.model.llm.api_endpoint,
      api_key: config.model.llm.api_key ?? "",
      api_style: (config.model.llm.api_style ?? "open_ai_chat_completions") as LlmApiStyle,
      stream: Boolean(config.model.llm.stream ?? false),
      supports_multimodal_input: Boolean(config.model.llm.supports_multimodal_input ?? false),
      timeout_secs: config.model.llm.timeout_secs,
      retry_count: config.model.llm.retry_count,
    },
  };
}

export function toolFormFromConfig(tool: AgentToolConfig): ToolFormState {
  const form = defaultToolForm();
  form.id = tool.id;
  form.name = tool.name;
  form.description = tool.description;
  form.enabled = tool.enabled;
  const nodeGraph = tool.tool_type as Record<string, unknown>;
  const targetType = String(nodeGraph.target_type ?? "workflow_set") as ToolTargetType;
  form.targetType = targetType;
  form.parametersJson = JSON.stringify(nodeGraph.parameters ?? [], null, 2);
  form.outputsJson = JSON.stringify(nodeGraph.outputs ?? [], null, 2);
  if (targetType === "workflow_set") {
    form.workflowName = String(nodeGraph.name ?? "");
  } else if (targetType === "file_path") {
    form.filePath = String(nodeGraph.path ?? "");
  } else if (targetType === "inline_graph") {
    form.inlineGraphJson = JSON.stringify(
      nodeGraph.graph ?? {
        nodes: [],
        edges: [],
        graph_inputs: [],
        graph_outputs: [],
        hyperparameter_groups: [],
        hyperparameters: [],
        variables: [],
        metadata: { name: null, description: null, version: null },
      },
      null,
      2,
    );
  }
  return form;
}

export function agentFormFromConfig(agent: AgentWithRuntime | AgentConfig): AgentFormState {
  const form = defaultAgentForm();
  form.id = agent.config_id;
  form.name = agent.name;
  form.enabled = agent.enabled;
  form.auto_start = agent.auto_start;
  form.is_default = agent.is_default;
  form.tools = agent.tools.map(toolFormFromConfig);
  const agentType = agent.agent_type as Record<string, unknown>;
  form.type = String(agentType.type) as AgentTypeName;
  if (form.type === "qq_chat") {
    form.ims_bot_adapter_connection_id = String(agentType.ims_bot_adapter_connection_id ?? "");
    form.rustfs_connection_id = String(agentType.rustfs_connection_id ?? "");
    form.bot_name = String(agentType.bot_name ?? "");
    form.system_prompt = String(agentType.system_prompt ?? "");
    form.llm_ref_id = String(agentType.llm_ref_id ?? "");
    form.intent_llm_ref_id = String(agentType.intent_llm_ref_id ?? "");
    form.math_programming_llm_ref_id = String(agentType.math_programming_llm_ref_id ?? "");
    form.embedding_model_ref_id = String(agentType.embedding_model_ref_id ?? "");
    form.tavily_connection_id = String(agentType.tavily_connection_id ?? "");
    form.mysql_connection_id = String(agentType.mysql_connection_id ?? "");
    form.weaviate_image_connection_id = String(agentType.weaviate_image_connection_id ?? "");
    form.max_message_length = Number(agentType.max_message_length ?? 500);
    form.compact_context_length = Number(agentType.compact_context_length ?? 0);
    const source = (agentType.default_tools_enabled ?? {}) as Record<string, unknown>;
    form.default_tools_enabled = defaultQqChatDefaultToolsEnabled();
    for (const tool of QQ_CHAT_DEFAULT_TOOLS) {
      const value = source[tool.id];
      if (typeof value === "boolean") {
        form.default_tools_enabled[tool.id] = value;
      }
    }
  } else {
    form.http_bind = String(agentType.bind ?? "127.0.0.1:18080");
    form.http_api_key = String(agentType.api_key ?? "");
    form.llm_ref_id = String(agentType.llm_ref_id ?? "");
  }
  return form;
}

export function buildToolPayload(form: ToolFormState): AgentToolConfig {
  const parameters = JSON.parse(form.parametersJson || "[]");
  const outputs = JSON.parse(form.outputsJson || "[]");
  let toolType: Record<string, unknown> & { type: string };
  if (form.targetType === "workflow_set") {
    toolType = {
      type: "node_graph",
      target_type: "workflow_set",
      name: form.workflowName.trim(),
      parameters,
      outputs,
    };
  } else if (form.targetType === "file_path") {
    toolType = {
      type: "node_graph",
      target_type: "file_path",
      path: form.filePath.trim(),
      parameters,
      outputs,
    };
  } else {
    toolType = {
      type: "node_graph",
      target_type: "inline_graph",
      graph: JSON.parse(form.inlineGraphJson || "{}"),
      parameters,
      outputs,
    };
  }
  return {
    id: form.id,
    name: form.name.trim(),
    description: form.description.trim(),
    enabled: form.enabled,
    tool_type: toolType,
  };
}

export function buildAgentPayload(form: AgentFormState): {
  name: string;
  enabled: boolean;
  auto_start: boolean;
  is_default: boolean;
  agent_type: Record<string, unknown>;
  tools: AgentToolConfig[];
} {
  const tools = form.tools.map(buildToolPayload);
  const common = {
    name: form.name.trim(),
    enabled: form.enabled,
    auto_start: form.auto_start,
    is_default: form.is_default,
    tools,
  };
  if (form.type === "qq_chat") {
    const defaultToolsEnabled = Object.fromEntries(
      QQ_CHAT_DEFAULT_TOOLS.map((tool) => [tool.id, form.default_tools_enabled[tool.id] !== false]),
    );
    return {
      ...common,
      agent_type: {
        type: "qq_chat",
        ims_bot_adapter_connection_id: form.ims_bot_adapter_connection_id,
        rustfs_connection_id: form.rustfs_connection_id || null,
        bot_name: form.bot_name.trim(),
        system_prompt: form.system_prompt.trim() || null,
        llm_ref_id: form.llm_ref_id || null,
        intent_llm_ref_id: form.intent_llm_ref_id || null,
        math_programming_llm_ref_id: form.math_programming_llm_ref_id || null,
        embedding_model_ref_id: form.embedding_model_ref_id || null,
        tavily_connection_id: form.tavily_connection_id,
        embedding: null,
        mysql_connection_id: form.mysql_connection_id || null,
        weaviate_image_connection_id: form.weaviate_image_connection_id || null,
        max_message_length: form.max_message_length,
        compact_context_length: form.compact_context_length,
        default_tools_enabled: defaultToolsEnabled,
      },
    };
  }

  return {
    ...common,
    agent_type: {
      type: "http_stream",
      bind: form.http_bind.trim(),
      api_key: form.http_api_key.trim() || null,
      llm_ref_id: form.llm_ref_id || null,
    },
  };
}

export function buildModelRefPayload(form: LlmFormState): ModelRefSpec {
  if (form.model_type === "text_embedding_local") {
    return {
      type: "text_embedding_local",
      model_name: form.local_model_name.trim(),
    };
  }

  return {
    type: "chat_llm",
    llm: {
      ...form.llm,
      api_key: form.llm.api_key?.trim() || null,
    },
  };
}

export function formatTime(value: string | null | undefined): string {
  if (!value) {
    return "未记录";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}

export function compactId(value: string | null | undefined): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未记录";
  }
  if (text.length <= 12) {
    return text;
  }
  return `${text.slice(0, 8)}...`;
}

export function summarizeIds(values: Array<string | null | undefined>): string {
  const ids = values.map((value) => String(value ?? "").trim()).filter(Boolean);
  if (ids.length === 0) {
    return "无";
  }
  if (ids.length === 1) {
    return compactId(ids[0]);
  }
  return `${compactId(ids[0])}, 等${ids.length}个`;
}

export function statusTone(status: string): string {
  switch (status) {
    case "running":
      return "running";
    case "starting":
      return "starting";
    case "success":
      return "success";
    case "error":
    case "failed":
      return "error";
    default:
      return "idle";
  }
}
