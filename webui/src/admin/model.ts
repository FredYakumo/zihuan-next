import type {
  AgentConfig,
  AgentToolConfig,
  AgentWithRuntime,
  ConnectionConfig,
  LlmConfig,
  LlmServiceConfig,
} from "../api/client";

export type ConnectionType = "mysql" | "redis" | "weaviate" | "rustfs" | "bot_adapter" | "ims_bot_adapter" | "tavily";
export type AgentTypeName = "qq_chat" | "http_stream";
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
  rustfs_endpoint: string;
  rustfs_bucket: string;
  rustfs_region: string;
  rustfs_access_key: string;
  rustfs_secret_key: string;
  rustfs_public_base_url: string;
  rustfs_path_style: boolean;
  bot_server_url: string;
  bot_server_token: string;
  qq_id: string;
  tavily_api_token: string;
  tavily_timeout_secs: number;
}

export interface LlmFormState {
  id: string | null;
  name: string;
  enabled: boolean;
  llm: LlmServiceConfig;
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
  llm_ref_id: string;
  tavily_connection_id: string;
  mysql_connection_id: string;
  weaviate_connection_id: string;
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
  { id: "get_recent_group_messages", label: "get_recent_group_messages", description: "查询群近期消息" },
  { id: "get_recent_user_messages", label: "get_recent_user_messages", description: "查询用户近期消息" },
  { id: "search_similar_messages", label: "search_similar_messages", description: "语义检索相似文本消息" },
  { id: "search_similar_images", label: "search_similar_images", description: "语义检索相似图片" },
  { id: "reply_plain_text", label: "reply_plain_text", description: "发送纯文本回复" },
  { id: "reply_at", label: "reply_at", description: "发送 @ 回复（群聊）" },
  { id: "reply_combine_text", label: "reply_combine_text", description: "发送组合消息（at + 文本）" },
  { id: "reply_forward_text", label: "reply_forward_text", description: "发送转发长文" },
  { id: "reply_send_image", label: "reply_send_image", description: "发送图片回复" },
  { id: "no_reply", label: "no_reply", description: "本轮不回复" },
];

export function defaultQqChatDefaultToolsEnabled(): Record<string, boolean> {
  return Object.fromEntries(QQ_CHAT_DEFAULT_TOOLS.map((tool) => [tool.id, true]));
}

export function defaultLlmConfig(): LlmServiceConfig {
  return {
    model_name: "",
    api_endpoint: "",
    api_key: "",
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
    rustfs_endpoint: "",
    rustfs_bucket: "",
    rustfs_region: "",
    rustfs_access_key: "",
    rustfs_secret_key: "",
    rustfs_public_base_url: "",
    rustfs_path_style: true,
    bot_server_url: "",
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
    llm: defaultLlmConfig(),
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
    inlineGraphJson: "{\n  \"nodes\": [],\n  \"edges\": []\n}",
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
    llm_ref_id: "",
    tavily_connection_id: "",
    mysql_connection_id: "",
    weaviate_connection_id: "",
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
  form.id = connection.id;
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
  return {
    id: config.id,
    name: config.name,
    enabled: config.enabled,
    llm: {
      model_name: config.llm.model_name,
      api_endpoint: config.llm.api_endpoint,
      api_key: config.llm.api_key ?? "",
      supports_multimodal_input: Boolean(config.llm.supports_multimodal_input ?? false),
      timeout_secs: config.llm.timeout_secs,
      retry_count: config.llm.retry_count,
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
    form.inlineGraphJson = JSON.stringify(nodeGraph.graph ?? { nodes: [], edges: [] }, null, 2);
  }
  return form;
}

export function agentFormFromConfig(agent: AgentWithRuntime | AgentConfig): AgentFormState {
  const form = defaultAgentForm();
  form.id = agent.id;
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
    form.llm_ref_id = String(agentType.llm_ref_id ?? "");
    form.tavily_connection_id = String(agentType.tavily_connection_id ?? "");
    form.mysql_connection_id = String(agentType.mysql_connection_id ?? "");
    form.weaviate_connection_id = String(agentType.weaviate_connection_id ?? "");
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
        llm_ref_id: form.llm_ref_id || null,
        tavily_connection_id: form.tavily_connection_id,
        embedding: null,
        mysql_connection_id: form.mysql_connection_id || null,
        weaviate_connection_id: form.weaviate_connection_id || null,
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
