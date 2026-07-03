import type {
  ServiceConfig,
  ServiceToolConfig,
  ServiceWithRuntime,
  ConnectionConfig,
  LlmConfig,
  ModelRefSpec,
  LlmServiceConfig,
} from "../api/client";

export type ConnectionType =
  | "mysql"
  | "redis"
  | "weaviate"
  | "rustfs"
  | "bot_adapter"
  | "ims_bot_adapter"
  | "web_search_engine"
  | "tokenizer"
  | "sqlite";
export type WeaviateCollectionSchema = "image_semantic" | "agent_memory";
export type ServiceTypeName = "qq_chat" | "http_stream" | "workspace";

/** Service types that support the Dashboard embedded Chat component. */
export const CHAT_ELIGIBLE_SERVICE_TYPES: ReadonlySet<string> = new Set(["http_stream", "workspace"]);
export type ModelRefType = "chat_llm" | "text_embedding_local";
export type ToolRunDuration = "Short" | "Long";
export type LlmApiStyle =
  | "candle_gguf"
  | "candle_hf"
  | "open_ai_chat_completions"
  | "open_ai_chat_completions_tencent_multimodal_compat"
  | "open_ai_responses"
  | "open_ai_responses_message_compat"
  | "open_ai_responses_image_url_object_compat";
export type ToolTargetType = "workflow_set" | "file_path" | "inline_graph";

export const DEFAULT_MYSQL_MAX_CONNECTIONS = 32;
export const DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS = 30;

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
  mysql_max_connections: number;
  mysql_acquire_timeout_secs: number;
  redis_url: string;
  redis_username: string;
  redis_password: string;
  weaviate_base_url: string;
  weaviate_class_name: string;
  weaviate_username: string;
  weaviate_password: string;
  weaviate_api_key: string;
  weaviate_collection_schema: WeaviateCollectionSchema;
  rustfs_endpoint: string;
  rustfs_username: string;
  rustfs_password: string;
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
  web_search_engine_provider: string;
  web_search_engine_api_token: string;
  web_search_engine_timeout_secs: number;
  tokenizer_model_name: string;
  sqlite_path: string;
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
  runDuration: ToolRunDuration;
  targetType: ToolTargetType;
  workflowName: string;
  filePath: string;
  inlineGraphJson: string;
  parametersJson: string;
  outputsJson: string;
}

export interface ServiceFormState {
  id: string | null;
  name: string;
  enabled: boolean;
  auto_start: boolean;
  is_default: boolean;
  type: ServiceTypeName;
  ims_bot_adapter_connection_id: string;
  rustfs_connection_id: string;
  bot_name: string;
  system_prompt: string;
  llm_ref_id: string;
  image_understand_llm_ref_id: string;
  intent_classification_llm_ref_id: string;
  math_programming_llm_ref_id: string;
  natural_language_reply_llm_ref_id: string;
  natural_language_reply_system_prompt: string;
  embedding_model_ref_id: string;
  tokenizer_connection_id: string;
  web_search_engine_connection_id: string;
  rdb_id: string;
  weaviate_image_connection_id: string;
  weaviate_memory_connection_id: string;
  max_message_length: number;
  compact_context_length: number;
  max_steer_count: number;
  emotion_dimensions: QqChatEmotionDimensionFormItem[];
  default_tools_enabled: Record<string, boolean>;
  tool_session_call_limits: Record<string, number>;
  tool_session_limit_message: string;
  message_rate_limit_default_enabled: boolean;
  message_rate_limit_default: QqChatMessageRateLimitRuleFormItem;
  message_rate_limit_groups: QqChatMessageRateLimitGroupFormItem[];
  message_rate_limit_users: QqChatMessageRateLimitUserFormItem[];
  http_bind: string;
  http_api_key: string;
  http_web_search_engine_connection_id: string;
  http_embedding_model_ref_id: string;
  http_weaviate_memory_connection_id: string;
  task_db_connection_id: string;
  tools: ToolFormState[];
  avatar_url: string;
}

export type DefaultToolOption = {
  id: string;
  label: string;
  description: string;
};

export type QqChatEmotionDimensionFormItem = {
  name: string;
  increase_weight?: number;
  decrease_weight?: number;
  positive_prompt?: string;
  negative_prompt?: string;
};

export type QqChatMessageRateLimitWindowUnit = "minute" | "hour" | "day";

export type QqChatMessageRateLimitRuleFormItem = {
  unlimited: boolean;
  window_unit: QqChatMessageRateLimitWindowUnit;
  window_size: number;
  max_calls: number;
};

export type QqChatMessageRateLimitGroupFormItem =
  QqChatMessageRateLimitRuleFormItem & {
    group_id: string;
  };

export type QqChatMessageRateLimitUserFormItem =
  QqChatMessageRateLimitRuleFormItem & {
    sender_id: string;
  };

export function isBotAdapterConnectionType(
  type: string,
): type is "bot_adapter" | "ims_bot_adapter" {
  return type === "bot_adapter" || type === "ims_bot_adapter";
}

export const QQ_CHAT_DEFAULT_TOOLS: DefaultToolOption[] = [
  { id: "web_search", label: "web_search", description: "联网搜索" },
  {
    id: "get_agent_public_info",
    label: "get_agent_public_info",
    description: "返回智能体公开信息",
  },
  {
    id: "get_function_list",
    label: "get_function_list",
    description: "获取功能列表",
  },
  {
    id: "get_recent_group_messages",
    label: "get_recent_group_messages",
    description: "只看群里最近几条消息，不适合按时段分析",
  },
  {
    id: "get_recent_user_messages",
    label: "get_recent_user_messages",
    description: "查询用户近期消息",
  },
  {
    id: "search_similar_images",
    label: "search_similar_images",
    description: "语义检索相似图片",
  },
  {
    id: "save_image",
    label: "save_image",
    description: "保存图片到图片库",
  },
  {
    id: "image_understand",
    label: "image_understand",
    description: "按 media_id 理解图片内容",
  },
  {
    id: "list_available_memory_keys",
    label: "list_available_memory_keys",
    description: "列出当前可访问的记忆标题",
  },
  {
    id: "search_memory_content",
    label: "search_memory_content",
    description: "搜索当前可访问的记忆内容",
  },
  {
    id: "remember_content",
    label: "remember_content",
    description: "把内容整理后写入记忆",
  },
];

export const HTTP_STREAM_DEFAULT_TOOLS: DefaultToolOption[] = [
  { id: "web_search", label: "web_search", description: "联网搜索" },
  {
    id: "list_available_memory_keys",
    label: "list_available_memory_keys",
    description: "列出当前可访问的记忆标题",
  },
  {
    id: "search_memory_content",
    label: "search_memory_content",
    description: "搜索当前可访问的记忆内容",
  },
  {
    id: "remember_content",
    label: "remember_content",
    description: "把内容整理后写入记忆",
  },
];

export const WORKSPACE_DEFAULT_TOOLS: DefaultToolOption[] = [
  { id: "create_file", label: "create_file", description: "创建文件" },
  { id: "delete_file", label: "delete_file", description: "删除文件" },
  { id: "edit_file", label: "edit_file", description: "按行替换文件内容" },
  { id: "exec_cmd", label: "exec_cmd", description: "执行命令" },
  { id: "ask_user", label: "ask_user", description: "向用户询问细节" },
];

export function defaultQqChatDefaultToolsEnabled(): Record<string, boolean> {
  return Object.fromEntries(
    QQ_CHAT_DEFAULT_TOOLS.map((tool) => [tool.id, true]),
  );
}

export function defaultQqChatEmotionDimensions(): QqChatEmotionDimensionFormItem[] {
  return [
    { name: "开心", increase_weight: 1, decrease_weight: 1 },
    { name: "烦恼", increase_weight: 1, decrease_weight: 1 },
    { name: "生气", increase_weight: 1, decrease_weight: 1 },
    { name: "伤心", increase_weight: 1, decrease_weight: 1 },
    { name: "害怕", increase_weight: 1, decrease_weight: 1 },
    { name: "焦虑", increase_weight: 1, decrease_weight: 1 },
    { name: "激动", increase_weight: 1, decrease_weight: 1 },
  ];
}

export function defaultQqChatMessageRateLimitRule(): QqChatMessageRateLimitRuleFormItem {
  return {
    unlimited: false,
    window_unit: "day",
    window_size: 1,
    max_calls: 20,
  };
}
export function defaultHttpStreamDefaultToolsEnabled(): Record<
  string,
  boolean
> {
  return Object.fromEntries(
    HTTP_STREAM_DEFAULT_TOOLS.map((tool) => [tool.id, true]),
  );
}

export function defaultWorkspaceDefaultToolsEnabled(): Record<string, boolean> {
  return Object.fromEntries(
    WORKSPACE_DEFAULT_TOOLS.map((tool) => [tool.id, true]),
  );
}

export function defaultLlmConfig(): LlmServiceConfig {
  return {
    model_name: "",
    api_endpoint: "",
    api_key: "",
    api_style: "open_ai_chat_completions",
    stream: false,
    supports_multimodal_input: false,
    include_reasoning_content: false,
    thinking_type: null,
    reasoning_effort: null,
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
    mysql_max_connections: DEFAULT_MYSQL_MAX_CONNECTIONS,
    mysql_acquire_timeout_secs: DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS,
    redis_url: "",
    redis_username: "",
    redis_password: "",
    weaviate_base_url: "",
    weaviate_class_name: "",
    weaviate_username: "",
    weaviate_password: "",
    weaviate_api_key: "",
    weaviate_collection_schema: "agent_memory",
    rustfs_endpoint: "",
    rustfs_username: "",
    rustfs_password: "",
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
    web_search_engine_provider: "tavily",
    web_search_engine_api_token: "",
    web_search_engine_timeout_secs: 30,
    tokenizer_model_name: "",
    sqlite_path: "",
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
    runDuration: "Short",
    targetType: "workflow_set",
    workflowName: "",
    filePath: "",
    inlineGraphJson:
      '{\n  "nodes": [],\n  "edges": [],\n  "graph_inputs": [],\n  "graph_outputs": [],\n  "hyperparameter_groups": [],\n  "hyperparameters": [],\n  "variables": [],\n  "metadata": { "name": null, "description": null, "version": null }\n}',
    parametersJson: "[]",
    outputsJson: "[]",
  };
}

export function defaultServiceForm(): ServiceFormState {
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
    image_understand_llm_ref_id: "",
    intent_classification_llm_ref_id: "",
    math_programming_llm_ref_id: "",
    natural_language_reply_llm_ref_id: "",
    natural_language_reply_system_prompt: "",
    embedding_model_ref_id: "",
    tokenizer_connection_id: "",
    web_search_engine_connection_id: "",
    rdb_id: "",
    weaviate_image_connection_id: "",
    weaviate_memory_connection_id: "",
    max_message_length: 500,
    compact_context_length: 0,
    max_steer_count: 4,
    emotion_dimensions: defaultQqChatEmotionDimensions(),
    default_tools_enabled: defaultQqChatDefaultToolsEnabled(),
    tool_session_call_limits: {},
    tool_session_limit_message: "",
    message_rate_limit_default_enabled: false,
    message_rate_limit_default: defaultQqChatMessageRateLimitRule(),
    message_rate_limit_groups: [],
    message_rate_limit_users: [],
    http_bind: "127.0.0.1:18080",
    http_api_key: "",
    http_web_search_engine_connection_id: "",
    http_embedding_model_ref_id: "",
    http_weaviate_memory_connection_id: "",
    task_db_connection_id: "",
    tools: [],
    avatar_url: "",
  };
}

function normalizeQqChatMessageRateLimitRule(
  value: unknown,
): QqChatMessageRateLimitRuleFormItem | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const source = value as Record<string, unknown>;
  if (source.unlimited === true) {
    return {
      unlimited: true,
      window_unit: "day",
      window_size: 1,
      max_calls: 1,
    };
  }
  const windowUnit = String(source.window_unit ?? "").trim();
  const maxCalls = Number(source.max_calls ?? 0);
  const rawWindowSize = Number(source.window_size ?? 1);
  const windowSize =
    Number.isFinite(rawWindowSize) && rawWindowSize > 0 ? Math.trunc(rawWindowSize) : 1;
  if (
    (windowUnit !== "minute" && windowUnit !== "hour" && windowUnit !== "day") ||
    !Number.isFinite(maxCalls) ||
    maxCalls <= 0
  ) {
    return null;
  }
  return {
    unlimited: false,
    window_unit: windowUnit as QqChatMessageRateLimitWindowUnit,
    window_size: windowSize,
    max_calls: maxCalls,
  };
}

function buildQqChatMessageRateLimitRulePayload(
  value: QqChatMessageRateLimitRuleFormItem,
): Record<string, unknown> {
  const windowSize =
    Number.isFinite(value.window_size) && value.window_size > 0
      ? Math.trunc(value.window_size)
      : 1;
  if (value.unlimited) {
    return {
      unlimited: true,
      window_unit: null,
      window_size: 1,
      max_calls: null,
    };
  }
  return {
    unlimited: false,
    window_unit: value.window_unit,
    window_size: windowSize,
    max_calls: Number.isFinite(value.max_calls) && value.max_calls > 0 ? value.max_calls : null,
  };
}

export function connectionFormFromConfig(
  connection: ConnectionConfig,
): ConnectionFormState {
  const form = defaultConnectionForm();
  form.id = connection.config_id;
  form.name = connection.name;
  form.enabled = connection.enabled;
  form.type = isBotAdapterConnectionType(String(connection.kind.type ?? ""))
    ? "bot_adapter"
    : (connection.kind.type as ConnectionType);
  switch (connection.kind.type) {
    case "mysql":
      form.mysql_url = String(connection.kind.url ?? "");
      form.mysql_max_connections = Number(
        connection.kind.max_connections ?? DEFAULT_MYSQL_MAX_CONNECTIONS,
      );
      form.mysql_acquire_timeout_secs = Number(
        connection.kind.acquire_timeout_secs ??
          DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS,
      );
      applyMysqlUrlToForm(form, form.mysql_url);
      break;
    case "redis":
      form.redis_url = String(connection.kind.url ?? "");
      form.redis_username = String(connection.kind.username ?? "");
      form.redis_password = String(connection.kind.password ?? "");
      break;
    case "weaviate":
      form.weaviate_base_url = String(connection.kind.base_url ?? "");
      form.weaviate_class_name = String(connection.kind.class_name ?? "");
      form.weaviate_username = String(connection.kind.username ?? "");
      form.weaviate_password = String(connection.kind.password ?? "");
      form.weaviate_api_key = String(connection.kind.api_key ?? "");
      form.weaviate_collection_schema = String(
        connection.kind.collection_schema ?? "agent_memory",
      ) as WeaviateCollectionSchema;
      break;
    case "rustfs":
      form.rustfs_endpoint = String(connection.kind.endpoint ?? "");
      form.rustfs_username = String(connection.kind.username ?? "");
      form.rustfs_password = String(connection.kind.password ?? "");
      form.rustfs_bucket = String(connection.kind.bucket ?? "");
      form.rustfs_region = String(connection.kind.region ?? "");
      form.rustfs_access_key = String(connection.kind.access_key ?? "");
      form.rustfs_secret_key = String(connection.kind.secret_key ?? "");
      form.rustfs_public_base_url = String(
        connection.kind.public_base_url ?? "",
      );
      form.rustfs_path_style = Boolean(connection.kind.path_style ?? false);
      break;
    case "bot_adapter":
    case "ims_bot_adapter":
      form.bot_server_url = String(connection.kind.bot_server_url ?? "");
      form.adapter_server_url = String(
        connection.kind.adapter_server_url ?? "",
      );
      form.bot_server_token = String(connection.kind.bot_server_token ?? "");
      form.qq_id = String(connection.kind.qq_id ?? "");
      break;
    case "web_search_engine":
      form.web_search_engine_provider = String(
        connection.kind.provider ?? "tavily",
      );
      form.web_search_engine_api_token = String(
        connection.kind.api_token ?? "",
      );
      form.web_search_engine_timeout_secs = Number(
        connection.kind.timeout_secs ?? 30,
      );
      break;
    case "tokenizer":
      form.tokenizer_model_name = String(connection.kind.model_name ?? "");
      break;
    case "sqlite":
      form.sqlite_path = String(connection.kind.path ?? "");
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
    form.mysql_database = decodeURIComponent(
      parsed.pathname.replace(/^\//, ""),
    );
  } catch {
    // Keep the raw URL for backward compatibility if parsing fails.
  }
}

function buildMysqlUrl(form: ConnectionFormState): string {
  if (
    form.mysql_host ||
    form.mysql_user ||
    form.mysql_password ||
    form.mysql_database
  ) {
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
      payload.kind = {
        type: "mysql",
        url: buildMysqlUrl(form),
        max_connections: form.mysql_max_connections,
        acquire_timeout_secs: form.mysql_acquire_timeout_secs,
      };
      break;
    case "redis":
      payload.kind = {
        type: "redis",
        url: form.redis_url.trim(),
        username: form.redis_username.trim() || null,
        password: form.redis_password.trim() || null,
      };
      break;
    case "weaviate":
      payload.kind = {
        type: "weaviate",
        base_url: form.weaviate_base_url.trim(),
        class_name: form.weaviate_class_name.trim(),
        username: form.weaviate_username.trim() || null,
        password: form.weaviate_password.trim() || null,
        api_key: form.weaviate_api_key.trim() || null,
        collection_schema: form.weaviate_collection_schema,
      };
      break;
    case "rustfs":
      payload.kind = {
        type: "rustfs",
        endpoint: form.rustfs_endpoint.trim(),
        username: form.rustfs_username.trim() || null,
        password: form.rustfs_password.trim() || null,
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
    case "web_search_engine":
      payload.kind = {
        type: "web_search_engine",
        provider: form.web_search_engine_provider,
        api_token: form.web_search_engine_api_token.trim() || null,
        timeout_secs: form.web_search_engine_timeout_secs,
      };
      break;
    case "tokenizer":
      payload.kind = {
        type: "tokenizer",
        model_name: form.tokenizer_model_name.trim(),
      };
      break;
    case "sqlite":
      payload.kind = {
        type: "sqlite",
        path: form.sqlite_path.trim(),
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
      api_style: (config.model.llm.api_style ??
        "open_ai_chat_completions") as LlmApiStyle,
      stream: Boolean(config.model.llm.stream ?? false),
      supports_multimodal_input: Boolean(
        config.model.llm.supports_multimodal_input ?? false,
      ),
      include_reasoning_content: Boolean(
        config.model.llm.include_reasoning_content ?? false,
      ),
      thinking_type: config.model.llm.thinking_type ?? null,
      reasoning_effort: config.model.llm.reasoning_effort ?? null,
      timeout_secs: config.model.llm.timeout_secs,
      retry_count: config.model.llm.retry_count,
    },
  };
}

export function toolFormFromConfig(tool: ServiceToolConfig): ToolFormState {
  const form = defaultToolForm();
  form.id = tool.id;
  form.name = tool.name;
  form.description = tool.description;
  form.enabled = tool.enabled;
  form.runDuration = (tool.run_duration ?? "Short") as ToolRunDuration;
  const nodeGraph = tool.tool_type as Record<string, unknown>;
  const targetType = String(
    nodeGraph.target_type ?? "workflow_set",
  ) as ToolTargetType;
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

export function serviceFormFromConfig(
  agent: ServiceWithRuntime | ServiceConfig,
): ServiceFormState {
  const form = defaultServiceForm();
  form.id = agent.config_id;
  form.name = agent.name;
  form.enabled = agent.enabled;
  form.auto_start = agent.auto_start;
  form.is_default = agent.is_default;
  form.tools = agent.tools.map(toolFormFromConfig);
  const agentType = agent.agent_type as Record<string, unknown>;
  form.type = String(agentType.type) as ServiceTypeName;
  if (form.type === "qq_chat") {
    form.ims_bot_adapter_connection_id = String(
      agentType.ims_bot_adapter_connection_id ?? "",
    );
    form.rustfs_connection_id = String(agentType.rustfs_connection_id ?? "");
    form.bot_name = String(agentType.bot_name ?? "");
    form.system_prompt = String(agentType.system_prompt ?? "");
    form.llm_ref_id = String(agentType.llm_ref_id ?? "");
    form.image_understand_llm_ref_id = String(
      agentType.image_understand_llm_ref_id ?? "",
    );
    form.intent_classification_llm_ref_id = String(
      agentType.intent_classification_llm_ref_id ?? "",
    );
    form.math_programming_llm_ref_id = String(
      agentType.math_programming_llm_ref_id ?? "",
    );
    form.natural_language_reply_llm_ref_id = String(
      agentType.natural_language_reply_llm_ref_id ?? "",
    );
    form.natural_language_reply_system_prompt = String(
      agentType.natural_language_reply_system_prompt ?? "",
    );
    form.embedding_model_ref_id = String(
      agentType.embedding_model_ref_id ?? "",
    );
    form.tokenizer_connection_id = String(
      agentType.tokenizer_connection_id ?? "",
    );
    form.web_search_engine_connection_id = String(
      agentType.web_search_engine_connection_id ?? "",
    );
    form.rdb_id = String(
      agentType.rdb_id ??
        agentType.mysql_connection_id ??
        agentType.task_db_connection_id ??
        "",
    );
    form.weaviate_image_connection_id = String(
      agentType.weaviate_image_connection_id ?? "",
    );
    form.weaviate_memory_connection_id = String(
      agentType.weaviate_memory_connection_id ?? "",
    );
    form.max_message_length = Number(agentType.max_message_length ?? 500);
    form.compact_context_length = Number(agentType.compact_context_length ?? 0);
    form.max_steer_count = Number(agentType.max_steer_count ?? 4);
    form.emotion_dimensions = normalizeQqChatEmotionDimensions(
      agentType.emotion_dimensions,
    );
    const source = (agentType.default_tools_enabled ?? {}) as Record<
      string,
      unknown
    >;
    form.default_tools_enabled = defaultQqChatDefaultToolsEnabled();
    for (const tool of QQ_CHAT_DEFAULT_TOOLS) {
      const value = source[tool.id];
      if (typeof value === "boolean") {
        form.default_tools_enabled[tool.id] = value;
      }
    }
    const limitsSource = (agentType.tool_session_call_limits ?? {}) as Record<
      string,
      unknown
    >;
    form.tool_session_call_limits = {};
    for (const [key, val] of Object.entries(limitsSource)) {
      const num = Number(val);
      if (Number.isFinite(num) && num > 0) {
        form.tool_session_call_limits[key] = num;
      }
    }
    form.tool_session_limit_message = String(
      agentType.tool_session_limit_message ?? "",
    );
    const defaultMessageRateLimit = normalizeQqChatMessageRateLimitRule(
      agentType.message_rate_limit_default,
    );
    form.message_rate_limit_default_enabled = Boolean(defaultMessageRateLimit);
    form.message_rate_limit_default =
      defaultMessageRateLimit ?? defaultQqChatMessageRateLimitRule();
    form.message_rate_limit_groups = Array.isArray(agentType.message_rate_limit_groups)
      ? agentType.message_rate_limit_groups
          .map((item) => {
            if (!item || typeof item !== "object") {
              return null;
            }
            const groupId = String((item as Record<string, unknown>).group_id ?? "").trim();
            const limit = normalizeQqChatMessageRateLimitRule(item);
            if (!groupId || !limit) {
              return null;
            }
            return { group_id: groupId, ...limit };
          })
          .filter((item): item is QqChatMessageRateLimitGroupFormItem => item != null)
      : [];
    form.message_rate_limit_users = Array.isArray(agentType.message_rate_limit_users)
      ? agentType.message_rate_limit_users
          .map((item) => {
            if (!item || typeof item !== "object") {
              return null;
            }
            const senderId = String((item as Record<string, unknown>).sender_id ?? "").trim();
            const limit = normalizeQqChatMessageRateLimitRule(item);
            if (!senderId || !limit) {
              return null;
            }
            return { sender_id: senderId, ...limit };
          })
          .filter((item): item is QqChatMessageRateLimitUserFormItem => item != null)
      : [];
  } else if (form.type === "http_stream") {
    form.http_bind = String(agentType.bind ?? "127.0.0.1:18080");
    form.http_api_key = String(agentType.api_key ?? "");
    form.llm_ref_id = String(agentType.llm_ref_id ?? "");
    form.http_web_search_engine_connection_id = String(
      agentType.web_search_engine_connection_id ?? "",
    );
    form.http_embedding_model_ref_id = String(
      agentType.embedding_model_ref_id ?? "",
    );
    form.http_weaviate_memory_connection_id = String(
      agentType.weaviate_memory_connection_id ?? "",
    );
    form.task_db_connection_id = String(agentType.task_db_connection_id ?? "");
    const source = (agentType.default_tools_enabled ?? {}) as Record<
      string,
      unknown
    >;
    form.default_tools_enabled = defaultHttpStreamDefaultToolsEnabled();
    for (const tool of HTTP_STREAM_DEFAULT_TOOLS) {
      const value = source[tool.id];
      if (typeof value === "boolean") {
        form.default_tools_enabled[tool.id] = value;
      }
    }
  } else {
    form.llm_ref_id = String(agentType.llm_ref_id ?? "");
    const source = (agentType.default_tools_enabled ?? {}) as Record<
      string,
      unknown
    >;
    form.default_tools_enabled = defaultWorkspaceDefaultToolsEnabled();
    for (const tool of WORKSPACE_DEFAULT_TOOLS) {
      const value = source[tool.id];
      if (typeof value === "boolean") {
        form.default_tools_enabled[tool.id] = value;
      }
    }
  }
  // avatar_url is at root level for http_stream and Workspace Agent Services
  form.avatar_url = String((agent as ServiceWithRuntime).avatar_url ?? "");
  return form;
}

export function buildToolPayload(form: ToolFormState): ServiceToolConfig {
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
    run_duration: form.runDuration,
    tool_type: toolType,
  };
}

export function buildServicePayload(form: ServiceFormState): {
  name: string;
  enabled: boolean;
  auto_start: boolean;
  is_default: boolean;
  agent_type: Record<string, unknown>;
  tools: ServiceToolConfig[];
  avatar_url?: string | null;
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
      QQ_CHAT_DEFAULT_TOOLS.map((tool) => [
        tool.id,
        form.default_tools_enabled[tool.id] !== false,
      ]),
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
        image_understand_llm_ref_id: form.image_understand_llm_ref_id || null,
        intent_classification_llm_ref_id:
          form.intent_classification_llm_ref_id || null,
        math_programming_llm_ref_id: form.math_programming_llm_ref_id || null,
        natural_language_reply_llm_ref_id:
          form.natural_language_reply_llm_ref_id || null,
        natural_language_reply_system_prompt:
          form.natural_language_reply_system_prompt.trim() || null,
        embedding_model_ref_id: form.embedding_model_ref_id || null,
        tokenizer_connection_id: form.tokenizer_connection_id || null,
        web_search_engine_connection_id: form.web_search_engine_connection_id,
        embedding: null,
        rdb_id: form.rdb_id || null,
        weaviate_image_connection_id: form.weaviate_image_connection_id || null,
        weaviate_memory_connection_id:
          form.weaviate_memory_connection_id || null,
        max_message_length: form.max_message_length,
        compact_context_length: form.compact_context_length,
        max_steer_count: form.max_steer_count,
        emotion_dimensions: normalizeQqChatEmotionDimensions(
          form.emotion_dimensions,
        ),
        default_tools_enabled: defaultToolsEnabled,
        tool_session_call_limits: Object.fromEntries(
          Object.entries(form.tool_session_call_limits)
            .filter(([, v]) => Number.isFinite(v) && v > 0)
            .map(([k, v]) => [k, Math.trunc(v)]),
        ),
        tool_session_limit_message:
          form.tool_session_limit_message.trim() || null,
        message_rate_limit_default: form.message_rate_limit_default_enabled
          ? buildQqChatMessageRateLimitRulePayload(form.message_rate_limit_default)
          : null,
        message_rate_limit_groups: form.message_rate_limit_groups
          .map((item) => ({
            group_id: item.group_id.trim(),
            ...buildQqChatMessageRateLimitRulePayload(item),
          }))
          .filter((item) => item.group_id),
        message_rate_limit_users: form.message_rate_limit_users
          .map((item) => ({
            sender_id: item.sender_id.trim(),
            ...buildQqChatMessageRateLimitRulePayload(item),
          }))
          .filter((item) => item.sender_id),
      },
    };
  }

  if (form.type === "http_stream") {
    return {
      ...common,
      avatar_url: form.avatar_url.trim() || null,
      agent_type: {
        type: "http_stream",
        bind: form.http_bind.trim(),
        api_key: form.http_api_key.trim() || null,
        llm_ref_id: form.llm_ref_id || null,
        embedding_model_ref_id: form.http_embedding_model_ref_id || null,
        web_search_engine_connection_id:
          form.http_web_search_engine_connection_id || null,
        weaviate_memory_connection_id:
          form.http_weaviate_memory_connection_id || null,
        task_db_connection_id: form.task_db_connection_id || null,
        default_tools_enabled: Object.fromEntries(
          HTTP_STREAM_DEFAULT_TOOLS.map((tool) => [
            tool.id,
            form.default_tools_enabled[tool.id] !== false,
          ]),
        ),
      },
    };
  }

  return {
    ...common,
    avatar_url: form.avatar_url.trim() || null,
    agent_type: {
      type: "workspace",
      llm_ref_id: form.llm_ref_id || null,
      default_tools_enabled: Object.fromEntries(
        WORKSPACE_DEFAULT_TOOLS.map((tool) => [
          tool.id,
          form.default_tools_enabled[tool.id] !== false,
        ]),
      ),
    },
  };
}

function normalizeQqChatEmotionDimensions(
  rawValue: unknown,
): QqChatEmotionDimensionFormItem[] {
  if (rawValue == null) {
    return defaultQqChatEmotionDimensions();
  }

  if (!Array.isArray(rawValue)) {
    throw new Error("情绪维度配置必须是数组");
  }

  const normalized = rawValue.map((item, index) => {
    if (!item || typeof item !== "object") {
      throw new Error(`情绪维度配置第 ${index + 1} 项必须是对象`);
    }

    const name = String((item as Record<string, unknown>).name ?? "").trim();
    if (!name) {
      throw new Error(`情绪维度配置第 ${index + 1} 项缺少 name`);
    }

    const increaseWeight = Number(
      (item as Record<string, unknown>).increase_weight ?? 1,
    );
    const decreaseWeight = Number(
      (item as Record<string, unknown>).decrease_weight ?? 1,
    );
    if (!Number.isFinite(increaseWeight) || increaseWeight <= 0) {
      throw new Error(`情绪维度 '${name}' 的 increase_weight 必须大于 0`);
    }
    if (!Number.isFinite(decreaseWeight) || decreaseWeight <= 0) {
      throw new Error(`情绪维度 '${name}' 的 decrease_weight 必须大于 0`);
    }

    const positivePrompt = String(
      (item as Record<string, unknown>).positive_prompt ?? "",
    ).trim();
    const negativePrompt = String(
      (item as Record<string, unknown>).negative_prompt ?? "",
    ).trim();

    return {
      name,
      increase_weight: increaseWeight,
      decrease_weight: decreaseWeight,
      ...(positivePrompt ? { positive_prompt: positivePrompt } : {}),
      ...(negativePrompt ? { negative_prompt: negativePrompt } : {}),
    };
  });

  if (normalized.length === 0) {
    throw new Error("至少需要配置一个情绪维度");
  }

  const nameSet = new Set<string>();
  for (const item of normalized) {
    if (nameSet.has(item.name)) {
      throw new Error(`情绪维度 '${item.name}' 重复了`);
    }
    nameSet.add(item.name);
  }

  return normalized;
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
      api_endpoint:
        form.llm.api_style === "candle_gguf" || form.llm.api_style === "candle_hf"
          ? ""
          : form.llm.api_endpoint.trim(),
      api_key:
        form.llm.api_style === "candle_gguf" || form.llm.api_style === "candle_hf"
          ? null
          : form.llm.api_key?.trim() || null,
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

export function getAvatarDisplayUrl(avatarUrl: string): string {
  if (!avatarUrl) {
    return "";
  }
  if (avatarUrl.startsWith("avatar://")) {
    const avatarId = avatarUrl.substring(9);
    return `/api/system/services/avatar/${avatarId}`;
  }
  return avatarUrl;
}

export function agentAvatarUrl(
  agent: ServiceWithRuntime | null | undefined,
): string {
  if (!agent) {
    return "";
  }

  const persisted = getAvatarDisplayUrl(String(agent.avatar_url ?? ""));
  if (persisted) {
    return persisted;
  }

  if (agent.agent_type.type === "qq_chat") {
    const explicit = String(agent.qq_chat_profile?.bot_avatar_url ?? "").trim();
    if (explicit) {
      return explicit;
    }
  }

  return "";
}

export function agentInitial(name: string): string {
  return (name || "B").trim().slice(0, 1).toUpperCase();
}
