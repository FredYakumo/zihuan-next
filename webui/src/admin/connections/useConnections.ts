import { computed, onMounted, reactive, ref } from "vue";

import {
  ApiError,
  fileIO,
  system,
  type ConnectionConfig,
  type RuntimeConnectionInstanceSummary,
} from "../../api/client";
import {
  assertConnectionConfig,
  buildConnectionPayload,
  compactId,
  connectionFormFromConfig,
  DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS,
  DEFAULT_MYSQL_MAX_CONNECTIONS,
  defaultConnectionForm,
  formatTime,
  isBotAdapterConnectionType,
  summarizeIds,
  type ConnectionFormState,
} from "../model";
import { useAdminClipboard } from "../components/useAdminClipboard";

type ConnectionTypeOption = {
  value: ConnectionFormState["type"];
  label: string;
  hint: string;
};

type ConnectionFilters = {
  keyword: string;
  type: ConnectionFormState["type"] | "all";
  enabled: "all" | "enabled" | "disabled";
};

export function useConnections() {
  const connectionTypes: ConnectionTypeOption[] = [
    { value: "mysql", label: "MySQL", hint: "数据库连接" },
    { value: "redis", label: "Redis", hint: "缓存与会话" },
    { value: "weaviate", label: "Weaviate", hint: "向量检索" },
    { value: "elasticsearch", label: "Elasticsearch", hint: "混合检索与向量检索" },
    { value: "rustfs", label: "RustFS", hint: "对象存储" },
    { value: "bot_adapter", label: "Bot Adapter", hint: "Bot 服务接入" },
    { value: "web_search_engine", label: "Web Search Engine", hint: "网页搜索引擎配置" },
    { value: "tokenizer", label: "Tokenizer", hint: "分词模型" },
    { value: "sqlite", label: "SQLite", hint: "SQLite 数据库" },
  ];

  const connections = ref<ConnectionConfig[]>([]);
  const runtimeInstances = ref<RuntimeConnectionInstanceSummary[]>([]);
  const tokenizerModels = ref<string[]>([]);
  const form = reactive<ConnectionFormState>(defaultConnectionForm());
  const drawerVisible = ref(false);
  const filters = reactive<ConnectionFilters>({
    keyword: "",
    type: "all",
    enabled: "all",
  });
  const isCreating = computed(() => form.id === null);
  const filteredConnections = computed(() => {
    const keyword = filters.keyword.trim().toLowerCase();
    return connections.value.filter((connection) => {
      if (filters.type !== "all" && connection.kind.type !== filters.type) {
        return false;
      }
      if (filters.enabled === "enabled" && !connection.enabled) {
        return false;
      }
      if (filters.enabled === "disabled" && connection.enabled) {
        return false;
      }
      if (!keyword) {
        return true;
      }
      return [connection.name, connection.config_id, connection.kind.type]
        .join(" ")
        .toLowerCase()
        .includes(keyword);
    });
  });

  const { copiedId, copyConfig, handleFileChange } = useAdminClipboard<ConnectionConfig>({
    validate: assertConnectionConfig,
    onImport: (config) => {
      Object.assign(form, connectionFormFromConfig(config));
      form.id = null;
      form.name = `${form.name} 副本`;
      drawerVisible.value = true;
    },
    isEnabled: () => !drawerVisible.value,
  });

  function resetForm() {
    Object.assign(form, defaultConnectionForm());
  }

  function startCreate(initialType?: ConnectionFormState["type"]) {
    resetForm();
    if (initialType) {
      form.type = initialType;
    }
    drawerVisible.value = true;
  }

  function closeDrawer() {
    resetForm();
    drawerVisible.value = false;
  }

  async function load() {
    const [loadedConnections, runtimeResponse, tokenizerModelResponse] = await Promise.all([
      system.connections.list(),
      system.connections.listRuntimeInstances({ page: 1, page_size: 200 }),
      fileIO.listTokenizerModels(),
    ]);
    connections.value = loadedConnections;
    runtimeInstances.value = runtimeResponse.items;
    tokenizerModels.value = tokenizerModelResponse.models;
  }

  function editConnection(connection: ConnectionConfig) {
    Object.assign(form, connectionFormFromConfig(connection));
    drawerVisible.value = true;
  }

  function duplicateConnection(connection: ConnectionConfig) {
    Object.assign(form, connectionFormFromConfig(connection));
    form.id = null;
    form.name = `${form.name} 副本`;
    drawerVisible.value = true;
  }

  function clearInactiveConnectionCredentials(type: "weaviate" | "elasticsearch") {
    if (type === "weaviate") {
      if (form.weaviate_auth_method === "password") {
        form.weaviate_api_key = "";
      } else {
        form.weaviate_username = "";
        form.weaviate_password = "";
      }
      return;
    }
    if (form.elasticsearch_auth_method === "password") {
      form.elasticsearch_api_key = "";
    } else {
      form.elasticsearch_username = "";
      form.elasticsearch_password = "";
    }
  }

  async function submitForm() {
    if (!form.name.trim()) {
      alert("请填写连接名称");
      return;
    }
    if (form.type === "mysql") {
      if (!form.mysql_host.trim()) {
        alert("请填写 MySQL 地址");
        return;
      }
      if (!form.mysql_port.trim()) {
        alert("请填写 MySQL 端口");
        return;
      }
      if (!form.mysql_database.trim()) {
        alert("请填写 MySQL 数据库名");
        return;
      }
      if (!Number.isInteger(form.mysql_max_connections) || form.mysql_max_connections <= 0) {
        alert("请填写大于 0 的 MySQL 最大连接数");
        return;
      }
      if (
        !Number.isInteger(form.mysql_acquire_timeout_secs) ||
        form.mysql_acquire_timeout_secs <= 0
      ) {
        alert("请填写大于 0 的 MySQL 获取连接超时秒数");
        return;
      }
    }
    if (form.type === "weaviate") {
      if (!form.weaviate_base_url.trim()) {
        alert("请填写 Weaviate Base URL");
        return;
      }
      if (!form.weaviate_class_name.trim()) {
        alert("请填写 Weaviate Class Name");
        return;
      }
      if (form.weaviate_auth_method === "password") {
        if (!form.weaviate_username.trim() || !form.weaviate_password.trim()) {
          alert("请填写 Weaviate 用户名和密码");
          return;
        }
      } else if (!form.weaviate_api_key.trim()) {
        alert("请填写 Weaviate API Key");
        return;
      }
    }
    if (form.type === "elasticsearch") {
      if (!form.elasticsearch_base_url.trim() || !form.elasticsearch_index_name.trim()) {
        alert("请填写 Elasticsearch Base URL 和 Index Name");
        return;
      }
      if (!Number.isInteger(form.elasticsearch_vector_dimensions) || form.elasticsearch_vector_dimensions <= 0) {
        alert("请填写大于 0 的 Elasticsearch 向量维度");
        return;
      }
      if (form.elasticsearch_auth_method === "password") {
        if (!form.elasticsearch_username.trim() || !form.elasticsearch_password.trim()) {
          alert("请填写 Elasticsearch 用户名和密码");
          return;
        }
      } else if (!form.elasticsearch_api_key.trim()) {
        alert("请填写 Elasticsearch API Key");
        return;
      }
    }
    if (form.type === "tokenizer" && !form.tokenizer_model_name.trim()) {
      alert("请选择 Tokenizer 模型");
      return;
    }
    try {
      const payload = buildConnectionPayload(form);
      const result = await saveConnection(payload, false);
      if (!result) return;
      if (result.collection_created) {
        alert(`已自动创建 Weaviate collection: ${form.weaviate_class_name.trim()}`);
      }
      closeDrawer();
      await load();
    } catch (error) {
      console.error("连接保存失败", error);
      alert(`连接保存失败：${formatSaveErrorMessage(error)}`);
    }
  }

  async function saveConnection(
    payload: ReturnType<typeof buildConnectionPayload>,
    allowCreateCollection: boolean,
  ) {
    try {
      const requestPayload = {
        ...payload,
        allow_create_collection: allowCreateCollection,
      };
      if (form.id) {
        return await system.connections.update(form.id, requestPayload);
      }
      return await system.connections.create(requestPayload);
    } catch (error) {
      if (error instanceof ApiError && error.code === "weaviate_collection_missing") {
        const className = String(error.details.class_name ?? form.weaviate_class_name.trim());
        if (window.confirm(`Weaviate collection "${className}" 不存在，是否自动新建？`)) {
          return await saveConnection(payload, true);
        }
        return null;
      }
      if (form.type === "elasticsearch" && error instanceof ApiError && error.message.includes("does not exist")) {
        if (window.confirm(`Elasticsearch index "${form.elasticsearch_index_name.trim()}" 不存在，是否自动新建？`)) {
          return await saveConnection(payload, true);
        }
        return null;
      }
      throw error;
    }
  }

  function formatSaveErrorMessage(error: unknown): string {
    if (error instanceof ApiError) {
      const rawMessage = String(error.message || "").trim();
      const lowerMessage = rawMessage.toLowerCase();

      if (
        error.status === 401 ||
        lowerMessage.includes("anonymous access not enabled") ||
        lowerMessage.includes("authenticate through one of the available methods")
      ) {
        return "检索数据库鉴权失败：请检查所选认证方式的凭据是否正确。";
      }

      if (rawMessage) {
        return rawMessage;
      }
      return `请求失败（HTTP ${error.status}）`;
    }

    if (error instanceof Error && error.message.trim()) {
      return error.message;
    }

    return "未知错误，请查看浏览器控制台日志。";
  }

  async function removeConnection(id: string) {
    await system.connections.delete(id);
    if (form.id === id) {
      closeDrawer();
    }
    await load();
  }

  function summarizeConnection(connection: ConnectionConfig): Array<{ label: string; value: string }> {
    const base = [
      { label: "Config ID", value: compactId(connection.config_id) },
      { label: "实例", value: summarizeConnectionInstances(connection.config_id) },
    ];
    const kind = connection.kind as Record<string, unknown>;
    switch (kind.type) {
      case "mysql":
        return [
          ...base,
          ...summarizeMysqlUrl(String(kind.url ?? "")),
          {
            label: "最大连接数",
            value: String(kind.max_connections ?? DEFAULT_MYSQL_MAX_CONNECTIONS),
          },
          {
            label: "获取超时",
            value: `${String(kind.acquire_timeout_secs ?? DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS)}s`,
          },
        ];
      case "redis":
        return [...base, { label: "URL", value: String(kind.url ?? "") }];
      case "weaviate":
        return [
          ...base,
          { label: "Base URL", value: String(kind.base_url ?? "") },
          { label: "Class", value: String(kind.class_name ?? "") },
          { label: "API Key", value: String(kind.api_key ?? "") ? "已配置" : "未设置" },
          { label: "Schema", value: formatWeaviateSchema(String(kind.collection_schema ?? "")) },
        ];
      case "rustfs":
        return [
          ...base,
          { label: "Endpoint", value: String(kind.endpoint ?? "") },
          { label: "Bucket", value: String(kind.bucket ?? "") },
        ];
      case "bot_adapter":
      case "ims_bot_adapter":
        return [
          ...base,
          { label: "WS", value: String(kind.bot_server_url ?? "") },
          { label: "HTTP", value: String(kind.adapter_server_url ?? "") || "未设置（默认由 WS 推导）" },
          { label: "QQ", value: String(kind.qq_id ?? "") || "未设置" },
        ];
      case "web_search_engine":
        return [
          ...base,
          { label: "Provider", value: String(kind.provider ?? "tavily") },
          { label: "API Token", value: String(kind.api_token ?? "") ? "已配置" : "未设置" },
          { label: "Timeout", value: String(kind.timeout_secs ?? 30) },
        ];
      case "tokenizer":
        return [
          ...base,
          { label: "模型", value: String(kind.model_name ?? "") || "未设置" },
        ];
      default:
        return base;
    }
  }

  function connectionSummary(connection: ConnectionConfig): string {
    return summarizeConnection(connection)
      .filter((item) => item.label !== "Config ID" && item.label !== "实例")
      .slice(0, 2)
      .map((item) => `${item.label}: ${item.value}`)
      .join(" · ");
  }

  function runtimeInstanceCount(configId: string): number {
    return runtimeInstances.value.filter((item) => item.config_id === configId).length;
  }

  function formatWeaviateSchema(schema: string): string {
    if (schema === "image_semantic") return "图片语义";
    if (schema === "agent_memory") return "Agent 记忆";
    return schema || "未设置";
  }

  function summarizeConnectionInstances(configId: string): string {
    const ids = runtimeInstances.value
      .filter((item) => item.config_id === configId)
      .map((item) => item.instance_id);
    return summarizeIds(ids);
  }

  function summarizeMysqlUrl(rawUrl: string): Array<{ label: string; value: string }> {
    if (!rawUrl) {
      return [{ label: "URL", value: "" }];
    }
    try {
      const parsed = new URL(rawUrl);
      return [
        { label: "地址", value: decodeURIComponent(parsed.hostname ?? "") },
        { label: "端口", value: parsed.port || "3306" },
        { label: "账号", value: decodeURIComponent(parsed.username ?? "") || "未设置" },
        { label: "数据库", value: decodeURIComponent(parsed.pathname.replace(/^\//, "")) || "未设置" },
      ];
    } catch {
      return [{ label: "URL", value: rawUrl }];
    }
  }

  onMounted(() => {
    load().catch((error) => {
      console.error(error);
      alert(`连接配置加载失败: ${(error as Error).message}`);
    });
  });

  return {
    connections,
    filteredConnections,
    runtimeInstances,
    tokenizerModels,
    form,
    drawerVisible,
    isCreating,
    filters,
    connectionTypes,
    startCreate,
    closeDrawer,
    load,
    editConnection,
    duplicateConnection,
    submitForm,
    removeConnection,
    summarizeConnection,
    connectionSummary,
    runtimeInstanceCount,
    clearInactiveConnectionCredentials,
    formatTime,
    isBotAdapterConnectionType,
    copiedId,
    copyConfig,
    handleFileChange,
  };
}

export type UseConnectionsReturn = ReturnType<typeof useConnections>;
