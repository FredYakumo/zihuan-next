import { computed, onMounted, ref } from "vue";

import {
  explorer,
  system,
  type ConnectionConfig,
  type LlmConfig,
  type MysqlRecord,
  type QqChatMessageRateLimitUsageRow,
  type RedisKeyEntry,
  type RustfsObject,
  type ServiceWithRuntime,
  type WeaviateSearchResult,
} from "../../api/client";
import type { WeaviateCollectionSchema } from "../model";

type TabId = "mysql" | "redis" | "weaviate" | "qq_chat_rate_limit" | "rustfs";


export function useDataExplorer() {
  const tabs = [
    { id: "mysql" as const, label: "MySQL" },
    { id: "redis" as const, label: "Redis" },
    { id: "weaviate" as const, label: "Weaviate" },
    { id: "qq_chat_rate_limit" as const, label: "用户用量" },
    { id: "rustfs" as const, label: "RustFS" },
  ];

  const activeTab = ref<TabId>("mysql");
  const connections = ref<ConnectionConfig[]>([]);
  const llmRefs = ref<LlmConfig[]>([]);
  const services = ref<ServiceWithRuntime[]>([]);

  const mysqlGoto = ref(1);
  const redisGoto = ref(1);
  const rustfsGoto = ref(1);

  function switchTab(tab: TabId) {
    activeTab.value = tab;
  }

  const mysqlConnections = computed(() =>
    connections.value.filter((c) => c.kind.type === "mysql" && c.enabled)
  );
  const redisConnections = computed(() =>
    connections.value.filter((c) => c.kind.type === "redis" && c.enabled)
  );
  const weaviateConnections = computed(() =>
    connections.value.filter((c) => c.kind.type === "weaviate" && c.enabled)
  );
  const rustfsConnections = computed(() =>
    connections.value.filter((c) => c.kind.type === "rustfs" && c.enabled)
  );
  const embeddingModels = computed(() =>
    llmRefs.value.filter((item) => item.model.type === "text_embedding_local" && item.enabled)
  );
  const qqChatServices = computed(() =>
    services.value.filter((item) => item.agent_type.type === "qq_chat" && item.enabled)
  );

  const mysql = ref({
    connectionId: "",
    loading: false,
    searched: false,
    records: [] as MysqlRecord[],
    total: 0,
    page: 1,
    pageSize: 20,
    filters: {
      message_id: "",
      sender_id: "",
      sender_name: "",
      group_id: "",
      content: "",
      send_time_start: "",
      send_time_end: "",
    },
  });

  const mysqlTotalPages = computed(() =>
    Math.max(1, Math.ceil(mysql.value.total / mysql.value.pageSize))
  );

  function onMysqlConnectionChange() {
    mysql.value.records = [];
    mysql.value.total = 0;
    mysql.value.page = 1;
    mysql.value.searched = false;
  }

  async function searchMysql() {
    if (!mysql.value.connectionId) return;
    mysql.value.loading = true;
    mysql.value.searched = true;
    try {
      const f = mysql.value.filters;
      const res = await explorer.queryMysql({
        connection_id: mysql.value.connectionId,
        message_id: f.message_id || undefined,
        sender_id: f.sender_id || undefined,
        sender_name: f.sender_name || undefined,
        group_id: f.group_id || undefined,
        content: f.content || undefined,
        send_time_start: f.send_time_start || undefined,
        send_time_end: f.send_time_end || undefined,
        page: mysql.value.page,
        page_size: mysql.value.pageSize,
      });
      mysql.value.records = res.records;
      mysql.value.total = res.total;
    } catch (e: unknown) {
      alert((e as Error).message);
    } finally {
      mysql.value.loading = false;
    }
  }

  function mysqlPage(p: number) {
    mysql.value.page = Math.max(1, Math.min(p, mysqlTotalPages.value));
    mysqlGoto.value = mysql.value.page;
    searchMysql();
  }

  const redis = ref({
    connectionId: "",
    loading: false,
    searched: false,
    keys: [] as RedisKeyEntry[],
    total: 0,
    page: 1,
    pageSize: 20,
    pattern: "*",
    scanCursor: 0,
  });

  const redisTotalPages = computed(() =>
    Math.max(1, Math.ceil(redis.value.total / redis.value.pageSize))
  );

  function onRedisConnectionChange() {
    redis.value.keys = [];
    redis.value.total = 0;
    redis.value.page = 1;
    redis.value.scanCursor = 0;
    redis.value.searched = false;
  }

  async function searchRedis() {
    if (!redis.value.connectionId) return;
    redis.value.loading = true;
    redis.value.searched = true;
    try {
      const res = await explorer.queryRedis({
        connection_id: redis.value.connectionId,
        pattern: redis.value.pattern || undefined,
        page: redis.value.page,
        page_size: redis.value.pageSize,
        scan_cursor: redis.value.scanCursor || undefined,
      });
      redis.value.keys = res.keys;
      redis.value.total = res.total;
      redis.value.scanCursor = res.scan_cursor;
    } catch (e: unknown) {
      alert((e as Error).message);
    } finally {
      redis.value.loading = false;
    }
  }

  function redisPage(p: number) {
    redis.value.page = Math.max(1, Math.min(p, redisTotalPages.value));
    redisGoto.value = redis.value.page;
    searchRedis();
  }

  function formatTTL(ttl: number): string {
    if (ttl === -1) return "永不过期";
    if (ttl === -2) return "已过期";
    if (ttl < 60) return `${ttl}s`;
    if (ttl < 3600) return `${Math.floor(ttl / 60)}m ${ttl % 60}s`;
    const h = Math.floor(ttl / 3600);
    const m = Math.floor((ttl % 3600) / 60);
    return `${h}h ${m}m`;
  }

  const weaviate = ref({
    connectionId: "",
    embeddingModelRefId: "",
    loading: false,
    searched: false,
    query: "",
    limit: 10,
    className: "",
    collectionSchema: "agent_memory" as WeaviateCollectionSchema,
    items: [] as WeaviateSearchResult[],
  });

  const selectedWeaviateConnection = computed(
    () => connections.value.find((c) => c.config_id === weaviate.value.connectionId) ?? null
  );
  const selectedWeaviateClassName = computed(() =>
    typeof selectedWeaviateConnection.value?.kind.class_name === "string"
      ? selectedWeaviateConnection.value.kind.class_name
      : weaviate.value.className
  );
  const selectedWeaviateSchema = computed(() => {
    const schema = selectedWeaviateConnection.value?.kind.collection_schema;
    return typeof schema === "string" ? (schema as WeaviateCollectionSchema) : weaviate.value.collectionSchema;
  });
  const isImageWeaviateSchema = computed(() => selectedWeaviateSchema.value === "image_semantic");
  const isAgentMemorySchema = computed(() => selectedWeaviateSchema.value === "agent_memory");

  function onWeaviateConnectionChange() {
    weaviate.value.items = [];
    weaviate.value.className = "";
    weaviate.value.collectionSchema = "agent_memory";
    weaviate.value.searched = false;
    if (isAgentMemorySchema.value) {
      searchWeaviate();
    }
  }

  function onWeaviateEmbeddingChange() {
    weaviate.value.items = [];
    weaviate.value.searched = false;
  }

  async function searchWeaviate() {
    if (!weaviate.value.connectionId) {
      return;
    }
    if (weaviate.value.query.trim() && !weaviate.value.embeddingModelRefId) {
      alert("请选择 Text Embedding 模型");
      return;
    }
    weaviate.value.loading = true;
    weaviate.value.searched = true;
    try {
      const res = await explorer.queryWeaviate({
        connection_id: weaviate.value.connectionId,
        embedding_model_ref_id: weaviate.value.embeddingModelRefId || undefined,
        query: weaviate.value.query.trim() || undefined,
        limit: weaviate.value.limit,
      });
      weaviate.value.items = res.items;
      weaviate.value.className = res.class_name;
      weaviate.value.collectionSchema = res.collection_schema;
    } catch (e: unknown) {
      alert((e as Error).message);
    } finally {
      weaviate.value.loading = false;
    }
  }

  function readStringProperty(properties: Record<string, unknown>, key: string): string {
    const value = properties[key];
    return typeof value === "string" ? value : "";
  }

  function readStringListProperty(properties: Record<string, unknown>, key: string): string[] {
    const value = properties[key];
    if (!Array.isArray(value)) {
      return [];
    }
    return value.filter((item): item is string => typeof item === "string");
  }

  function formatWeaviateDistance(distance: number | null): string {
    return typeof distance === "number" ? distance.toFixed(4) : "—";
  }

  function stringifyWeaviateProperties(properties: Record<string, unknown>): string {
    return JSON.stringify(properties, null, 2);
  }

  function csvToList(value: string | null | undefined): string[] {
    return String(value ?? "")
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);
  }

  async function createAgentMemory() {
    if (!weaviate.value.connectionId) return;
    if (!weaviate.value.embeddingModelRefId) {
      alert("创建记忆前请先选择 Text Embedding 模型");
      return;
    }
    const title = window.prompt("记忆标题");
    if (!title?.trim()) return;
    const value = window.prompt("记忆内容");
    if (!value?.trim()) return;
    const expiresAt = window.prompt("过期时间 RFC3339，可留空表示永久");
    const senderCsv = window.prompt("sender_id_list，多个用逗号分隔，可留空");
    const groupCsv = window.prompt("group_id_list，多个用逗号分隔，可留空");
    await explorer.createAgentMemory(
      weaviate.value.connectionId,
      weaviate.value.embeddingModelRefId,
      {
        title: title.trim(),
        value: value.trim(),
        expires_at: expiresAt?.trim() || null,
        sender_id_list: csvToList(senderCsv),
        group_id_list: csvToList(groupCsv),
      },
    );
    await searchWeaviate();
  }

  async function editAgentMemory(item: WeaviateSearchResult) {
    if (!weaviate.value.connectionId || !item.object_id) return;
    if (!weaviate.value.embeddingModelRefId) {
      alert("编辑记忆前请先选择 Text Embedding 模型");
      return;
    }
    const title = window.prompt("记忆标题", readStringProperty(item.properties, "title"));
    if (!title?.trim()) return;
    const value = window.prompt("记忆内容", readStringProperty(item.properties, "value"));
    if (!value?.trim()) return;
    const expiresAt = window.prompt(
      "过期时间 RFC3339，可留空表示永久",
      readStringProperty(item.properties, "expires_at"),
    );
    const senderCsv = window.prompt(
      "sender_id_list，多个用逗号分隔，可留空",
      readStringListProperty(item.properties, "sender_id_list").join(","),
    );
    const groupCsv = window.prompt(
      "group_id_list，多个用逗号分隔，可留空",
      readStringListProperty(item.properties, "group_id_list").join(","),
    );
    await explorer.updateAgentMemory(
      weaviate.value.connectionId,
      weaviate.value.embeddingModelRefId,
      item.object_id,
      {
        title: title.trim(),
        value: value.trim(),
        expires_at: expiresAt?.trim() || null,
        sender_id_list: csvToList(senderCsv),
        group_id_list: csvToList(groupCsv),
      },
    );
    await searchWeaviate();
  }

  async function removeAgentMemory(item: WeaviateSearchResult) {
    if (!weaviate.value.connectionId || !item.object_id) return;
    if (!window.confirm("确认删除这条记忆吗？")) return;
    await explorer.deleteAgentMemory(weaviate.value.connectionId, item.object_id);
    await searchWeaviate();
  }

  const qqChatRateLimit = ref({
    agentId: "",
    loading: false,
    searched: false,
    resettingSenderId: "",
    items: [] as QqChatMessageRateLimitUsageRow[],
  });

  function onQqChatRateLimitAgentChange() {
    qqChatRateLimit.value.items = [];
    qqChatRateLimit.value.searched = false;
    if (qqChatRateLimit.value.agentId) {
      void loadQqChatRateLimitUsage();
    }
  }

  async function loadQqChatRateLimitUsage() {
    if (!qqChatRateLimit.value.agentId) return;
    qqChatRateLimit.value.loading = true;
    qqChatRateLimit.value.searched = true;
    try {
      const res = await explorer.queryQqChatRateLimitUsage(qqChatRateLimit.value.agentId);
      qqChatRateLimit.value.items = res.items;
    } catch (e: unknown) {
      alert((e as Error).message);
    } finally {
      qqChatRateLimit.value.loading = false;
    }
  }

  async function resetQqChatRateLimitUsage(senderId: string) {
    if (!qqChatRateLimit.value.agentId) return;
    if (!window.confirm(`确认清空用户 ${senderId} 的当前计数吗？`)) return;
    qqChatRateLimit.value.resettingSenderId = senderId;
    try {
      await explorer.resetQqChatRateLimitUsage(qqChatRateLimit.value.agentId, senderId);
      await loadQqChatRateLimitUsage();
    } catch (e: unknown) {
      alert((e as Error).message);
    } finally {
      qqChatRateLimit.value.resettingSenderId = "";
    }
  }

  function formatRateLimitScope(scopeType: string, scopeKey: string): string {
    if (scopeType === "user") return `用户: ${scopeKey}`;
    if (scopeType === "group") return `群组: ${scopeKey}`;
    return "默认";
  }

  function formatRateLimitWindow(windowUnit: string): string {
    if (windowUnit === "minute") return "分钟";
    if (windowUnit === "hour") return "小时";
    if (windowUnit === "day") return "天";
    return windowUnit || "—";
  }

  function formatRateLimitUsage(usedCalls: number, maxCalls: number | null, unlimited: boolean): string {
    if (unlimited) return `${usedCalls}/无限`;
    return `${usedCalls}/${maxCalls ?? "—"}`;
  }

  const rustfs = ref({
    connectionId: "",
    loading: false,
    searched: false,
    objects: [] as RustfsObject[],
    prefixes: [] as string[],
    total: 0,
    page: 1,
    pageSize: 20,
    prefix: "",
    search: "",
  });

  const rustfsTotalPages = computed(() =>
    Math.max(1, Math.ceil(rustfs.value.total / rustfs.value.pageSize))
  );

  function onRustfsConnectionChange() {
    rustfs.value.objects = [];
    rustfs.value.prefixes = [];
    rustfs.value.total = 0;
    rustfs.value.page = 1;
    rustfs.value.prefix = "";
    rustfs.value.searched = false;
  }

  async function searchRustfs() {
    if (!rustfs.value.connectionId) return;
    rustfs.value.loading = true;
    rustfs.value.searched = true;
    try {
      const res = await explorer.queryRustfs({
        connection_id: rustfs.value.connectionId,
        prefix: rustfs.value.prefix || undefined,
        search: rustfs.value.search || undefined,
        page: rustfs.value.page,
        page_size: rustfs.value.pageSize,
      });
      rustfs.value.objects = res.objects;
      rustfs.value.prefixes = res.prefixes;
      rustfs.value.total = res.total;
    } catch (e: unknown) {
      alert((e as Error).message);
    } finally {
      rustfs.value.loading = false;
    }
  }

  function rustfsPage(p: number) {
    rustfs.value.page = Math.max(1, Math.min(p, rustfsTotalPages.value));
    rustfsGoto.value = rustfs.value.page;
    searchRustfs();
  }

  function navigateRustfsPrefix(segmentIndex: number) {
    const parts = rustfs.value.prefix.split("/").filter(Boolean);
    rustfs.value.prefix = parts.slice(0, segmentIndex + 1).join("/") + "/";
    searchRustfs();
  }

  function shortKey(key: string): string {
    const idx = key.lastIndexOf("/");
    return idx >= 0 ? key.slice(idx + 1) : key;
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function onImageError(e: Event) {
    const img = e.target as HTMLImageElement;
    img.style.display = "none";
  }

  onMounted(async () => {
    const [connectionResult, llmResult, serviceResult] = await Promise.allSettled([
      system.connections.list(),
      system.llm.list(),
      system.services.list(),
    ]);
    if (connectionResult.status === "fulfilled") {
      connections.value = connectionResult.value;
    }
    if (llmResult.status === "fulfilled") {
      llmRefs.value = llmResult.value;
    }
    if (serviceResult.status === "fulfilled") {
      services.value = serviceResult.value;
    }
  });

  return {
    tabs,
    activeTab,
    switchTab,
    mysqlConnections,
    mysql,
    mysqlGoto,
    mysqlTotalPages,
    onMysqlConnectionChange,
    searchMysql,
    mysqlPage,
    redisConnections,
    redis,
    redisGoto,
    redisTotalPages,
    onRedisConnectionChange,
    searchRedis,
    redisPage,
    formatTTL,
    weaviateConnections,
    weaviate,
    embeddingModels,
    selectedWeaviateConnection,
    selectedWeaviateClassName,
    selectedWeaviateSchema,
    isImageWeaviateSchema,
    isAgentMemorySchema,
    onWeaviateConnectionChange,
    onWeaviateEmbeddingChange,
    searchWeaviate,
    readStringProperty,
    readStringListProperty,
    formatWeaviateDistance,
    stringifyWeaviateProperties,
    createAgentMemory,
    editAgentMemory,
    removeAgentMemory,
    qqChatServices,
    qqChatRateLimit,
    onQqChatRateLimitAgentChange,
    loadQqChatRateLimitUsage,
    resetQqChatRateLimitUsage,
    formatRateLimitScope,
    formatRateLimitWindow,
    formatRateLimitUsage,
    rustfsConnections,
    rustfs,
    rustfsGoto,
    rustfsTotalPages,
    onRustfsConnectionChange,
    searchRustfs,
    rustfsPage,
    navigateRustfsPrefix,
    shortKey,
    formatSize,
    onImageError,
  };
}

export type UseDataExplorerReturn = ReturnType<typeof useDataExplorer>;
