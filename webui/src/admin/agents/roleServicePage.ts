import { computed, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { system, type ServiceWithRuntime } from "../../api/client";
import { useAgents } from "./useAgents";
import { assertConnectionConfig, assertLlmConfig } from "../model";

export function useRoleServicePage() {
const agents = useAgents();
const {
  serviceTypes,
  services,
  servicesLoading,
  connections,
  llm,
  workflows,
  form,
  showCreatePicker,
  showCreateForm,
  showEditModal,
  showEmotionDimensionsModal,
  showRateLimitModal,
  showIgnoreRulesModal,
  ignoreRulesLoading,
  ignoreRules,
  ignoreRuleSubmitting,
  ignoreRuleDeletingId,
  ignoreRuleError,
  ignoreRuleForm,
  emotionDimensionAdding,
  emotionDimensionDraft,
  emotionDimensionEditingIndex,
  toolSearchQuery,
  toolRows,
  filteredToolRows,
  toolRowClassName,
  showDefaultToolEditModal,
  editingDefaultToolId,
  defaultToolEditDraft,
  currentEditingDefaultTool,
  openDefaultToolEditModal,
  closeDefaultToolEditModal,
  confirmDefaultToolEdit,
  chatModels,
  multimodalChatModels,
  embeddingModels,
  botConnections,
  rustfsConnections,
  webSearchEngineConnections,
  taskDbConnections,
  tokenizerConnections,
  imageWeaviateConnections,
  memoryWeaviateConnections,
  imageElasticsearchConnections,
  memoryElasticsearchConnections,
  retrievalConnections,
  ignoreRulesDisabledReason,
  resetForm,
  avatarUploading,
  handleAvatarFileSelect,
  uploadAvatarFile,
  clearAvatar,
  clearEditingAgent,
  ignoreRulePreview,
  formatRequestError,
  startCreate,
  closeCreatePicker,
  pickCreateType,
  closeEditor,
  load,
  editService,
  duplicateService,
  closeEditModal,
  openEmotionDimensionsModal,
  closeEmotionDimensionsModal,
  resetEmotionDimensionDraft,
  startAddEmotionDimension,
  cancelAddEmotionDimension,
  buildEmotionDimensionPayload,
  confirmAddEmotionDimension,
  editEmotionDimension,
  cancelEditEmotionDimension,
  confirmEditEmotionDimension,
  removeEmotionDimension,
  resetIgnoreRuleForm,
  formatIgnoreRule,
  loadIgnoreRules,
  openIgnoreRulesModal,
  closeIgnoreRulesModal,
  openRateLimitModal,
  editIgnoreRule,
  submitIgnoreRule,
  removeIgnoreRule,
  showToolEditModal,
  editingToolIndex,
  toolEditDraft,
  toolEditCallLimit,
  openNewTool,
  openToolEdit,
  closeToolEditModal,
  confirmToolEdit,
  removeTool,
  validateImageUnderstandModelSelection,
  isGeneratedToolId,
  syncingToolIndex,
  syncToolFromGraph,
  handleToolTargetTypeChange,
  applyWorkflowSetMetadata,
  submitForm,
  removeService,
  startAgent,
  stopAgent,
  toggleServiceRuntime,
  llmName,
  llmRefName,
  runtimeBadgeText,
  compactId,
  formatTime,
  statusTone,
  summarizeIds,
  getAvatarDisplayUrl,
  agentAvatarUrl,
  agentInitial,
  serviceCopiedId,
  copyServiceConfig,
  handleServiceFileChange,
} = agents;

const router = useRouter();
const showModelConfigDialog = ref(false);
const modelImporting = ref(false);
const showRetrievalDatabaseDialog = ref(false);
const retrievalDatabaseImporting = ref(false);
const showWebSearchDialog = ref(false);
const webSearchImporting = ref(false);

function handlePrimaryModelChange(value: string | number) {
  if (String(value) !== "__add_model__") return;
  form.llm_ref_id = "";
  showModelConfigDialog.value = true;
}

function handleImageUnderstandModelChange(value: string | number) {
  if (String(value) !== "__add_model__") return;
  form.image_understand_llm_ref_id = "";
  showModelConfigDialog.value = true;
}

function handleMemoryBackendChange(value: string | number) {
  if (String(value) !== "__add_retrieval_database__") return;
  form.workspace_memory_backend = "";
  showRetrievalDatabaseDialog.value = true;
}

function handleWebSearchChange(value: string | number) {
  if (String(value) !== "__add_web_search__") return;
  form.web_search_engine_connection_id = "";
  showWebSearchDialog.value = true;
}

function openModelCreatePage() {
  showModelConfigDialog.value = false;
  router.push({ path: "/llm", query: { action: "create" } });
}

function openRetrievalDatabaseCreatePage() {
  showRetrievalDatabaseDialog.value = false;
  router.push({ path: "/connections", query: { action: "create" } });
}

function openWebSearchCreatePage() {
  showWebSearchDialog.value = false;
  router.push({ path: "/connections", query: { action: "create", type: "web_search_engine" } });
}

async function importModelFromText(raw: string) {
  if (modelImporting.value) return;
  modelImporting.value = true;
  try {
    const config = assertLlmConfig(JSON.parse(raw));
    const created = await system.llm.create({ name: config.name, enabled: config.enabled, model: config.model });
    await load();
    form.llm_ref_id = created.config_id;
    showModelConfigDialog.value = false;
  } catch (error) {
    alert(`模型配置导入失败：${error instanceof Error ? error.message : String(error)}`);
  } finally {
    modelImporting.value = false;
  }
}

async function importModelFromClipboard() {
  try {
    await importModelFromText(await navigator.clipboard.readText());
  } catch (error) {
    alert(`读取剪贴板失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

function handleModelFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => { void importModelFromText(String(reader.result)); input.value = ""; };
  reader.onerror = () => { alert("文件读取失败"); input.value = ""; };
  reader.readAsText(file);
}

async function importRetrievalDatabaseFromText(raw: string) {
  if (retrievalDatabaseImporting.value) return;
  retrievalDatabaseImporting.value = true;
  try {
    const config = assertConnectionConfig(JSON.parse(raw));
    const type = String(config.kind.type);
    if (type !== "weaviate" && type !== "elasticsearch") {
      throw new Error("检索数据库仅支持 Weaviate 或 Elasticsearch 连接配置");
    }
    const created = await system.connections.create({ name: config.name, enabled: config.enabled, kind: config.kind });
    await load();
    form.workspace_memory_backend = type;
    if (type === "weaviate") form.workspace_weaviate_memory_connection_id = created.config_id;
    else form.workspace_elasticsearch_memory_connection_id = created.config_id;
    showRetrievalDatabaseDialog.value = false;
  } catch (error) {
    alert(`检索数据库导入失败：${error instanceof Error ? error.message : String(error)}`);
  } finally {
    retrievalDatabaseImporting.value = false;
  }
}

async function importRetrievalDatabaseFromClipboard() {
  try {
    await importRetrievalDatabaseFromText(await navigator.clipboard.readText());
  } catch (error) {
    alert(`读取剪贴板失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

function handleRetrievalDatabaseFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => { void importRetrievalDatabaseFromText(String(reader.result)); input.value = ""; };
  reader.onerror = () => { alert("文件读取失败"); input.value = ""; };
  reader.readAsText(file);
}

async function importWebSearchFromText(raw: string) {
  if (webSearchImporting.value) return;
  webSearchImporting.value = true;
  try {
    const config = assertConnectionConfig(JSON.parse(raw));
    if (config.kind.type !== "web_search_engine") {
      throw new Error("Web Search 仅支持 Web Search Engine 连接配置");
    }
    const created = await system.connections.create({ name: config.name, enabled: config.enabled, kind: config.kind });
    await load();
    form.web_search_engine_connection_id = created.config_id;
    showWebSearchDialog.value = false;
  } catch (error) {
    alert(`Web Search 导入失败：${error instanceof Error ? error.message : String(error)}`);
  } finally {
    webSearchImporting.value = false;
  }
}

async function importWebSearchFromClipboard() {
  try {
    await importWebSearchFromText(await navigator.clipboard.readText());
  } catch (error) {
    alert(`读取剪贴板失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

function handleWebSearchFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => { void importWebSearchFromText(String(reader.result)); input.value = ""; };
  reader.onerror = () => { alert("文件读取失败"); input.value = ""; };
  reader.readAsText(file);
}

const serviceImportFileInput = ref<HTMLInputElement | null>(null);
const filters = reactive({
  keyword: "",
  type: "all",
  status: "all",
});

const filteredServices = computed(() => {
  const keyword = filters.keyword.trim().toLowerCase();
  return services.value.filter((service) => {
    if (filters.type !== "all" && service.role_service_type.type !== filters.type) {
      return false;
    }
    if (filters.status !== "all" && service.runtime.status !== filters.status) {
      return false;
    }
    if (!keyword) {
      return true;
    }
    return `${service.name} ${service.config_id}`.toLowerCase().includes(keyword);
  });
});

const columns = [
  { colKey: "name", title: "Service 名称", width: 230 },
  { colKey: "type", title: "Service 类型", width: 185 },
  { colKey: "model", title: "模型配置", ellipsis: true },
  { colKey: "runtime", title: "运行状态", width: 150 },
  { colKey: "enabled", title: "配置状态", width: 100 },
  { colKey: "updated", title: "启动时间", width: 170 },
  { colKey: "actions", title: "操作", width: 310, fixed: "right" },
];

const toolColumns = [
  { colKey: "label", title: "工具名称", width: 150 },
  { colKey: "id", title: "工具 ID", width: 160 },
  { colKey: "description", title: "说明", ellipsis: true },
  { colKey: "enabled", title: "启用", width: 70 },
  { colKey: "actions", title: "操作", width: 130 },
];

function triggerServiceImportFile() {
  serviceImportFileInput.value?.click();
}

function serviceTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    qq_chat: "QQ Chat",
    workspace: "Workspace",
  };
  return labels[type] ?? type;
}

function runtimeTheme(status: string): "success" | "warning" | "danger" | "default" {
  if (status === "running") {
    return "success";
  }
  if (status === "starting") {
    return "warning";
  }
  if (status === "error") {
    return "danger";
  }
  return "default";
}

function copyServiceConfigItem(service: ServiceWithRuntime) {
  const payload = {
    name: service.name,
    enabled: service.enabled,
    auto_start: service.auto_start,
    is_default: service.is_default,
    role_service_type: service.role_service_type,
    tools: service.tools,
    ...(service.avatar_url ? { avatar_url: service.avatar_url } : {}),
  };
  copyServiceConfig(payload, service.config_id);
}

return {
  ...agents,
  showModelConfigDialog,
  modelImporting,
  showRetrievalDatabaseDialog,
  retrievalDatabaseImporting,
  showWebSearchDialog,
  webSearchImporting,
  handlePrimaryModelChange,
  handleImageUnderstandModelChange,
  handleMemoryBackendChange,
  handleWebSearchChange,
  openModelCreatePage,
  openRetrievalDatabaseCreatePage,
  openWebSearchCreatePage,
  importModelFromClipboard,
  handleModelFileChange,
  importRetrievalDatabaseFromClipboard,
  handleRetrievalDatabaseFileChange,
  importWebSearchFromClipboard,
  handleWebSearchFileChange,
  serviceImportFileInput,
  filters,
  filteredServices,
  columns,
  toolColumns,
  triggerServiceImportFile,
  serviceTypeLabel,
  runtimeTheme,
  copyServiceConfigItem,
};
}
