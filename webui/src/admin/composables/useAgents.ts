import { computed, onMounted, reactive, ref } from "vue";

import {
  system,
  workflows as workflowApi,
  type ServiceWithRuntime,
  type ConnectionConfig,
  type LlmConfig,
  type QqChatAgentServiceIgnoreRule,
  type WorkflowInfo,
} from "../../api/client";
import {
  serviceFormFromConfig,
  buildServicePayload,
  HTTP_STREAM_DEFAULT_TOOLS,
  WORKSPACE_DEFAULT_TOOLS,
  isBotAdapterConnectionType,
  QQ_CHAT_DEFAULT_TOOLS,
  defaultServiceForm,
  defaultHttpStreamDefaultToolsEnabled,
  defaultQqChatDefaultToolsEnabled,
  defaultToolForm,
  defaultQqChatMessageRateLimitRule,
  defaultWorkspaceDefaultToolsEnabled,
  compactId,
  formatTime,
  statusTone,
  summarizeIds,
  getAvatarDisplayUrl,
  agentAvatarUrl,
  agentInitial,
  type ServiceFormState,
  type ServiceTypeName,
  type QqChatEmotionDimensionFormItem,
} from "../model";

export function useAgents() {
type ServiceTypeOption = {
  value: ServiceTypeName;
  label: string;
  hint: string;
};

const serviceTypes: ServiceTypeOption[] = [
  {
    value: "qq_chat",
    label: "QQ Chat Agent Service",
    hint: "通过 QQ Bot Adapter 提供对话服务",
  },
  {
    value: "http_stream",
    label: "HTTP Stream Service",
    hint: "通过 HTTP 流式接口对外提供服务",
  },
  {
    value: "workspace",
    label: "Workspace Agent Service",
    hint: "面向项目目录的开发型 Agent Service",
  },
];

const services = ref<ServiceWithRuntime[]>([]);
const servicesLoading = ref(false);
const connections = ref<ConnectionConfig[]>([]);
const llm = ref<LlmConfig[]>([]);
const workflows = ref<WorkflowInfo[]>([]);
const form = reactive<ServiceFormState>(defaultServiceForm());
const editingServiceId = ref("");
const showCreatePicker = ref(false);
const showCreateForm = ref(false);
const showEditModal = ref(false);
const showEmotionDimensionsModal = ref(false);
const showRateLimitModal = ref(false);
const showIgnoreRulesModal = ref(false);
const ignoreRulesLoading = ref(false);
const ignoreRules = ref<QqChatAgentServiceIgnoreRule[]>([]);
const ignoreRuleSubmitting = ref(false);
const ignoreRuleDeletingId = ref<number | null>(null);
const ignoreRuleError = ref("");
const ignoreRuleForm = reactive<{
  id: number | null;
  sender_id: string;
  group_id: string;
}>({
  id: null,
  sender_id: "",
  group_id: "",
});
const emotionDimensionAdding = ref(false);
const emotionDimensionDraft = reactive<{
  name: string;
  increase_weight: number;
  decrease_weight: number;
  dissipation_hours: number;
  positive_prompt: string;
  negative_prompt: string;
}>({
  name: "",
  increase_weight: 1,
  decrease_weight: 1,
  dissipation_hours: 5,
  positive_prompt: "",
  negative_prompt: "",
});
const emotionDimensionEditingIndex = ref<number | null>(null);
const qqChatDefaultTools = QQ_CHAT_DEFAULT_TOOLS;
const httpStreamDefaultTools = HTTP_STREAM_DEFAULT_TOOLS;
const workspaceDefaultTools = WORKSPACE_DEFAULT_TOOLS;

const currentDefaultTools = computed(() => {
  if (form.type === "qq_chat") return qqChatDefaultTools;
  if (form.type === "http_stream") return httpStreamDefaultTools;
  if (form.type === "workspace") return workspaceDefaultTools;
  return [];
});

const defaultToolSearchQuery = ref("");

const filteredDefaultTools = computed(() => {
  const q = defaultToolSearchQuery.value.trim().toLowerCase();
  if (!q) return currentDefaultTools.value;
  return currentDefaultTools.value.filter(
    (t) =>
      t.label.toLowerCase().includes(q) ||
      t.id.toLowerCase().includes(q) ||
      t.description.toLowerCase().includes(q),
  );
});

const showDefaultToolEditModal = ref(false);
const editingDefaultToolId = ref("");
const defaultToolEditDraft = reactive({
  enabled: true,
  callLimit: 0 as number | null,
  imageUnderstandLlmRefId: "",
});

const currentEditingDefaultTool = computed(() =>
  currentDefaultTools.value.find((t) => t.id === editingDefaultToolId.value),
);

function openDefaultToolEditModal(toolId: string) {
  editingDefaultToolId.value = toolId;
  defaultToolEditDraft.enabled = form.default_tools_enabled[toolId] !== false;
  defaultToolEditDraft.callLimit = form.tool_session_call_limits[toolId] ?? null;
  defaultToolEditDraft.imageUnderstandLlmRefId = form.image_understand_llm_ref_id ?? "";
  showDefaultToolEditModal.value = true;
}

function closeDefaultToolEditModal() {
  showDefaultToolEditModal.value = false;
}

function confirmDefaultToolEdit() {
  const id = editingDefaultToolId.value;
  form.default_tools_enabled[id] = defaultToolEditDraft.enabled;
  if (defaultToolEditDraft.callLimit != null && defaultToolEditDraft.callLimit > 0) {
    form.tool_session_call_limits[id] = defaultToolEditDraft.callLimit;
  } else {
    delete form.tool_session_call_limits[id];
  }
  if (id === "image_understand") {
    form.image_understand_llm_ref_id = defaultToolEditDraft.imageUnderstandLlmRefId;
  }
  showDefaultToolEditModal.value = false;
}

const chatModels = computed(() =>
  llm.value.filter((item) => item.model.type === "chat_llm"),
);
const multimodalChatModels = computed(() =>
  llm.value.filter(
    (item) =>
      item.model.type === "chat_llm" &&
      Boolean(item.model.llm.supports_multimodal_input),
  ),
);
const embeddingModels = computed(() =>
  llm.value.filter(
    (item) => item.model.type === "text_embedding_local" && item.enabled,
  ),
);
const mainChatModel = computed(() =>
  llm.value.find((item) => item.config_id === form.llm_ref_id),
);
const mainChatModelSupportsMultimodal = computed(() => {
  const selected = mainChatModel.value;
  return Boolean(
    selected?.model.type === "chat_llm" &&
    selected.model.llm.supports_multimodal_input,
  );
});

const botConnections = computed(() =>
  connections.value.filter((item) =>
    isBotAdapterConnectionType(String(item.kind.type ?? "")),
  ),
);
const rustfsConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "rustfs"),
);
const webSearchEngineConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "web_search_engine"),
);
const taskDbConnections = computed(() =>
  connections.value.filter(
    (item) => item.kind.type === "mysql" || item.kind.type === "sqlite",
  ),
);
const tokenizerConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "tokenizer"),
);
const imageWeaviateConnections = computed(() =>
  connections.value.filter(
    (item) =>
      item.kind.type === "weaviate" &&
      item.kind.collection_schema === "image_semantic",
  ),
);
const memoryWeaviateConnections = computed(() =>
  connections.value.filter(
    (item) =>
      item.kind.type === "weaviate" &&
      item.kind.collection_schema === "agent_memory",
  ),
);
const ignoreRulesDisabledReason = computed(() => {
  if (!editingServiceId.value) {
    return "请先保存当前 Service，再管理 Ignore Rules。";
  }
  if (!form.rdb_id) {
    return "先配置 RDB Connection，Ignore Rules 和任务/消息持久化都会共用这条关系库连接。";
  }
  return "";
});

function resetForm() {
  Object.assign(form, defaultServiceForm());
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

const avatarUploading = ref(false);

function handleAvatarFileSelect(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;

  // Validate file type
  if (!file.type.startsWith('image/')) {
    alert('请上传图片文件');
    return;
  }

  // Validate file size (max 30MB)
  const maxSize = 30 * 1024 * 1024;
  if (file.size > maxSize) {
    alert('图片大小不能超过 30MB');
    return;
  }

  uploadAvatarFile(file);

  // Reset input
  input.value = '';
}

async function uploadAvatarFile(file: File) {
  if (avatarUploading.value) return;

  avatarUploading.value = true;
  try {
    const formData = new FormData();
    formData.append('file', file);

    const response = await fetch('/api/system/services/avatar', {
      method: 'POST',
      body: formData,
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(error || '上传失败');
    }

    const result = await response.json();
    if (result.avatar_id) {
      // Store avatar:// prefix to distinguish from external URLs
      form.avatar_url = `avatar://${result.avatar_id}`;
    }
  } catch (e) {
    alert(`头像上传失败: ${e}`);
  } finally {
    avatarUploading.value = false;
  }
}

function clearAvatar() {
  form.avatar_url = '';
}

// Get display URL for avatar (handles avatar:// prefix)
function clearEditingAgent() {
  editingServiceId.value = "";
}

const ignoreRulePreview = computed(() =>
  formatIgnoreRule(ignoreRuleForm.sender_id, ignoreRuleForm.group_id),
);

function formatRequestError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return "请求失败，请稍后重试";
}

function startCreate() {
  resetForm();
  clearEditingAgent();
  showCreatePicker.value = true;
  showCreateForm.value = false;
}

function closeCreatePicker() {
  resetForm();
  clearEditingAgent();
  showEmotionDimensionsModal.value = false;
  showCreatePicker.value = false;
  showCreateForm.value = false;
}

function pickCreateType(type: ServiceTypeName) {
  resetForm();
  clearEditingAgent();
  form.type = type;
  if (type === "qq_chat") {
    form.default_tools_enabled = defaultQqChatDefaultToolsEnabled();
    form.tool_session_call_limits = { web_search: 1 };
    form.tool_session_limit_message = "";
  } else if (type === "http_stream") {
    form.default_tools_enabled = defaultHttpStreamDefaultToolsEnabled();
    form.tool_session_call_limits = {};
    form.tool_session_limit_message = "";
  } else {
    form.default_tools_enabled = defaultWorkspaceDefaultToolsEnabled();
    form.tool_session_call_limits = {};
    form.tool_session_limit_message = "";
  }
  showCreatePicker.value = true;
  showCreateForm.value = true;
}

function closeEditor() {
  showCreatePicker.value = false;
  showCreateForm.value = false;
  closeEditModal();
}

async function load() {
  servicesLoading.value = true;
  try {
    const [loadedAgents, loadedConnections, loadedLlm, loadedWorkflows] =
      await Promise.all([
        system.services.list(),
        system.connections.list(),
        system.llm.list(),
        workflowApi.listDetailed(),
      ]);
    services.value = loadedAgents;
    connections.value = loadedConnections;
    llm.value = loadedLlm;
    workflows.value = loadedWorkflows.workflows;
  } finally {
    servicesLoading.value = false;
  }
}

function editService(service: ServiceWithRuntime) {
  Object.assign(form, serviceFormFromConfig(service));
  editingServiceId.value = service.config_id;
  showEditModal.value = true;
}

function closeEditModal() {
  showEmotionDimensionsModal.value = false;
  showEditModal.value = false;
  resetForm();
  clearEditingAgent();
}
function openEmotionDimensionsModal() {
  resetEmotionDimensionDraft();
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = null;
  showEmotionDimensionsModal.value = true;
}

function closeEmotionDimensionsModal() {
  showEmotionDimensionsModal.value = false;
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

function resetEmotionDimensionDraft() {
  emotionDimensionDraft.name = "";
  emotionDimensionDraft.increase_weight = 1;
  emotionDimensionDraft.decrease_weight = 1;
  emotionDimensionDraft.dissipation_hours = 5;
  emotionDimensionDraft.positive_prompt = "";
  emotionDimensionDraft.negative_prompt = "";
}

function startAddEmotionDimension() {
  resetEmotionDimensionDraft();
  emotionDimensionEditingIndex.value = null;
  emotionDimensionAdding.value = true;
}

function cancelAddEmotionDimension() {
  emotionDimensionAdding.value = false;
  resetEmotionDimensionDraft();
}

function buildEmotionDimensionPayload(): QqChatEmotionDimensionFormItem | null {
  const name = emotionDimensionDraft.name.trim();
  if (!name) {
    alert("请填写情绪维度名称");
    return null;
  }
  if (
    !Number.isFinite(emotionDimensionDraft.increase_weight) ||
    emotionDimensionDraft.increase_weight < 0
  ) {
    alert("升权重不能为负数");
    return null;
  }
  if (
    !Number.isFinite(emotionDimensionDraft.decrease_weight) ||
    emotionDimensionDraft.decrease_weight < 0
  ) {
    alert("降权重不能为负数");
    return null;
  }
  if (
    !Number.isInteger(emotionDimensionDraft.dissipation_hours) ||
    emotionDimensionDraft.dissipation_hours <= 0
  ) {
    alert("消解时间必须是正整数小时");
    return null;
  }
  return {
    name,
    increase_weight: emotionDimensionDraft.increase_weight,
    decrease_weight: emotionDimensionDraft.decrease_weight,
    dissipation_hours: emotionDimensionDraft.dissipation_hours,
    positive_prompt: emotionDimensionDraft.positive_prompt.trim() || undefined,
    negative_prompt: emotionDimensionDraft.negative_prompt.trim() || undefined,
  };
}

function confirmAddEmotionDimension() {
  const payload = buildEmotionDimensionPayload();
  if (!payload) {
    return;
  }

  const duplicateIndex = form.emotion_dimensions.findIndex(
    (item) => item.name.trim() === payload.name,
  );
  if (duplicateIndex >= 0) {
    alert(`情绪维度 '${payload.name}' 已存在`);
    return;
  }

  form.emotion_dimensions.unshift(payload);
  emotionDimensionAdding.value = false;
  resetEmotionDimensionDraft();
}

function editEmotionDimension(index: number) {
  const dimension = form.emotion_dimensions[index];
  if (!dimension) {
    return;
  }
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = index;
  emotionDimensionDraft.name = dimension.name;
  emotionDimensionDraft.increase_weight = Number(dimension.increase_weight ?? 1);
  emotionDimensionDraft.decrease_weight = Number(dimension.decrease_weight ?? 1);
  emotionDimensionDraft.dissipation_hours = Number(dimension.dissipation_hours ?? 5);
  emotionDimensionDraft.positive_prompt = dimension.positive_prompt ?? "";
  emotionDimensionDraft.negative_prompt = dimension.negative_prompt ?? "";
}

function cancelEditEmotionDimension() {
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

function confirmEditEmotionDimension() {
  if (emotionDimensionEditingIndex.value == null) {
    return;
  }
  const payload = buildEmotionDimensionPayload();
  if (!payload) {
    return;
  }

  const duplicateIndex = form.emotion_dimensions.findIndex(
    (item, index) =>
      item.name.trim() === payload.name &&
      index !== emotionDimensionEditingIndex.value,
  );
  if (duplicateIndex >= 0) {
    alert(`情绪维度 '${payload.name}' 已存在`);
    return;
  }

  form.emotion_dimensions.splice(emotionDimensionEditingIndex.value, 1, payload);
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

function removeEmotionDimension(index: number) {
  const dimension = form.emotion_dimensions[index];
  if (!dimension) {
    return;
  }
  if (!window.confirm(`确认删除情绪维度 '${dimension.name}' 吗？`)) {
    return;
  }
  form.emotion_dimensions.splice(index, 1);
  if (emotionDimensionEditingIndex.value === index) {
    emotionDimensionEditingIndex.value = null;
    resetEmotionDimensionDraft();
    return;
  }
  if (
    emotionDimensionEditingIndex.value != null &&
    emotionDimensionEditingIndex.value > index
  ) {
    emotionDimensionEditingIndex.value -= 1;
  }
}

function resetIgnoreRuleForm() {
  ignoreRuleForm.id = null;
  ignoreRuleForm.sender_id = "";
  ignoreRuleForm.group_id = "";
  ignoreRuleError.value = "";
}

function formatIgnoreRule(
  senderId: string | null | undefined,
  groupId: string | null | undefined,
): string {
  const sender = String(senderId ?? "").trim();
  const group = String(groupId ?? "").trim();
  if (sender && group) {
    return `屏蔽群 ${group} 下的 QQ ${sender}`;
  }
  if (sender) {
    return `屏蔽 QQ ${sender}`;
  }
  if (group) {
    return `屏蔽群 ${group}`;
  }
  return "至少填写 sender_id 或 group_id 其中一个";
}

async function loadIgnoreRules() {
  if (!editingServiceId.value) {
    return;
  }
  ignoreRulesLoading.value = true;
  try {
    ignoreRuleError.value = "";
    ignoreRules.value = await system.services.listIgnoreRules(
      editingServiceId.value,
    );
  } catch (error) {
    ignoreRuleError.value = `加载 Ignore Rules 失败: ${formatRequestError(error)}`;
  } finally {
    ignoreRulesLoading.value = false;
  }
}

async function openIgnoreRulesModal() {
  if (ignoreRulesDisabledReason.value) {
    alert(ignoreRulesDisabledReason.value);
    return;
  }
  resetIgnoreRuleForm();
  showIgnoreRulesModal.value = true;
  await loadIgnoreRules();
}

function closeIgnoreRulesModal() {
  showIgnoreRulesModal.value = false;
  resetIgnoreRuleForm();
  ignoreRuleDeletingId.value = null;
}

function openRateLimitModal() {
  showRateLimitModal.value = true;
}

function closeRateLimitModal() {
  showRateLimitModal.value = false;
}

function addGroupRateLimitRule() {
  form.message_rate_limit_groups.push({
    group_id: "",
    ...defaultQqChatMessageRateLimitRule(),
  });
}

function removeGroupRateLimitRule(index: number) {
  form.message_rate_limit_groups.splice(index, 1);
}

function addUserRateLimitRule() {
  form.message_rate_limit_users.push({
    sender_id: "",
    ...defaultQqChatMessageRateLimitRule(),
  });
}

function removeUserRateLimitRule(index: number) {
  form.message_rate_limit_users.splice(index, 1);
}

function editIgnoreRule(rule: QqChatAgentServiceIgnoreRule) {
  ignoreRuleForm.id = rule.id;
  ignoreRuleForm.sender_id = rule.sender_id ?? "";
  ignoreRuleForm.group_id = rule.group_id ?? "";
}

async function submitIgnoreRule() {
  if (!editingServiceId.value) {
    return;
  }
  const payload = {
    sender_id: ignoreRuleForm.sender_id.trim() || null,
    group_id: ignoreRuleForm.group_id.trim() || null,
  };
  if (!payload.sender_id && !payload.group_id) {
    alert("sender_id 和 group_id 至少填写一个");
    return;
  }
  ignoreRuleSubmitting.value = true;
  ignoreRuleError.value = "";
  try {
    if (ignoreRuleForm.id == null) {
      await system.services.createIgnoreRule(editingServiceId.value, payload);
    } else {
      await system.services.updateIgnoreRule(
        editingServiceId.value,
        ignoreRuleForm.id,
        payload,
      );
    }
    resetIgnoreRuleForm();
    await loadIgnoreRules();
  } catch (error) {
    ignoreRuleError.value = `保存 Ignore Rule 失败: ${formatRequestError(error)}`;
  } finally {
    ignoreRuleSubmitting.value = false;
  }
}

async function removeIgnoreRule(ruleId: number) {
  if (!editingServiceId.value) {
    return;
  }
  if (!window.confirm("确认删除这条 Ignore Rule 吗？")) {
    return;
  }
  ignoreRuleDeletingId.value = ruleId;
  ignoreRuleError.value = "";
  try {
    await system.services.deleteIgnoreRule(editingServiceId.value, ruleId);
    if (ignoreRuleForm.id === ruleId) {
      resetIgnoreRuleForm();
    }
    await loadIgnoreRules();
  } catch (error) {
    ignoreRuleError.value = `删除 Ignore Rule 失败: ${formatRequestError(error)}`;
  } finally {
    ignoreRuleDeletingId.value = null;
  }
}

function addTool() {
  form.tools.push(defaultToolForm());
}

function removeTool(index: number) {
  form.tools.splice(index, 1);
}

function validateImageUnderstandModelSelection(): string | null {
  if (form.type !== "qq_chat" || !form.default_tools_enabled.image_understand) {
    return null;
  }
  if (form.image_understand_llm_ref_id) {
    const selected = llm.value.find(
      (item) => item.config_id === form.image_understand_llm_ref_id,
    );
    if (
      !selected ||
      selected.model.type !== "chat_llm" ||
      !selected.model.llm.supports_multimodal_input
    ) {
      return "image_understand 需要选择一个支持多模态的模型";
    }
    return null;
  }
  if (!mainChatModelSupportsMultimodal.value) {
    return "image_understand 已启用时，主模型不支持多模态，请选择一个支持多模态的模型";
  }
  return null;
}

const RESERVED_TOOL_RUNTIME_INPUTS = new Set([
  "content",
  "message_event",
  "qq_ims_bot_adapter",
]);

function isGeneratedToolId(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
    value.trim(),
  );
}

const syncingToolIndex = ref<number | null>(null);

async function syncToolFromGraph(
  tool: ServiceFormState["tools"][number],
  index: number,
) {
  syncingToolIndex.value = index;
  try {
    const result = await workflowApi.listDetailed();
    workflows.value = result.workflows;
    applyWorkflowSetMetadata(tool);
  } finally {
    syncingToolIndex.value = null;
  }
}

function handleToolTargetTypeChange(tool: ServiceFormState["tools"][number]) {
  if (tool.implementation !== "node_graph") {
    return;
  }
  if (tool.targetType === "workflow_set" && tool.workflowName) {
    applyWorkflowSetMetadata(tool);
  }
}

function applyWorkflowSetMetadata(tool: ServiceFormState["tools"][number]) {
  if (tool.implementation !== "node_graph" || tool.targetType !== "workflow_set" || !tool.workflowName) {
    return;
  }
  const workflow = workflows.value.find(
    (item) => item.name === tool.workflowName,
  );
  if (!workflow) {
    return;
  }

  if (!tool.id.trim() || isGeneratedToolId(tool.id)) {
    tool.id = workflow.name;
  }
  tool.name = workflow.name;
  tool.description = workflow.description ?? "";
  tool.parametersJson = JSON.stringify(
    (workflow.inputs ?? [])
      .filter((port) => !RESERVED_TOOL_RUNTIME_INPUTS.has(port.name))
      .map((port) => ({
        name: port.name,
        data_type: port.data_type,
        desc: port.description ?? "",
      })),
    null,
    2,
  );
  tool.outputsJson = JSON.stringify(
    (workflow.outputs ?? []).map((port) => ({
      name: port.name,
      data_type: port.data_type,
      desc: port.description ?? "",
    })),
    null,
    2,
  );
}

async function submitForm() {
  try {
    const payload = buildServicePayload(form);
    if (!payload.name) {
      alert("请填写 Agent 名称");
      return;
    }
    if (!form.llm_ref_id) {
      alert("请绑定一个模型配置");
      return;
    }
    if (form.type === "qq_chat" && !form.ims_bot_adapter_connection_id) {
      alert("QQ Chat Agent Service 需要绑定 Bot Adapter");
      return;
    }
    if (form.type === "qq_chat" && !form.web_search_engine_connection_id) {
      alert("QQ Chat Agent Service 需要绑定 Web Search Engine 连接");
      return;
    }
    const imageUnderstandError = validateImageUnderstandModelSelection();
    if (imageUnderstandError) {
      alert(imageUnderstandError);
      return;
    }
    if (
      form.type === "http_stream" &&
      form.default_tools_enabled.web_search !== false &&
      !form.http_web_search_engine_connection_id
    ) {
      alert(
        "启用 web_search 时，HTTP stream service 需要绑定 Web Search Engine 连接",
      );
      return;
    }
    if (
      form.type === "qq_chat" &&
      form.weaviate_memory_connection_id &&
      !form.embedding_model_ref_id
    ) {
      alert("QQ Chat Agent Service 启用记忆库时需要绑定文本向量模型");
      return;
    }
    if (
      form.type === "http_stream" &&
      form.http_weaviate_memory_connection_id &&
      !form.http_embedding_model_ref_id
    ) {
      alert("HTTP stream service 启用记忆库时需要绑定文本向量模型");
      return;
    }
    if (form.id) {
      await system.services.update(form.id, payload);
    } else {
      await system.services.create(payload);
    }
    closeEditor();
    await load();
  } catch (error) {
    alert(`保存 Agent 失败: ${(error as Error).message}`);
  }
}

async function removeService(id: string) {
  if (!window.confirm("确认删除这个 Agent 吗？")) {
    return;
  }
  await system.services.delete(id);
  if (form.id === id) {
    closeEditor();
  }
  await load();
}

async function startAgent(id: string) {
  try {
    console.log(`[Agent] 启动 Agent ${id}`);
    await system.services.start(id);
    await load();
  } catch (error) {
    alert(`启动失败: ${(error as Error).message}`);
  }
}

async function stopAgent(id: string) {
  try {
    console.log(`[Agent] 停止 Agent ${id}`);
    await system.services.stop(id);
    await load();
  } catch (error) {
    alert(`停止失败: ${(error as Error).message}`);
  }
}

async function toggleServiceRuntime(service: ServiceWithRuntime) {
  if (service.runtime.status === "running") {
    await stopAgent(service.config_id);
  } else {
    await startAgent(service.config_id);
  }
}

function llmName(service: ServiceWithRuntime): string {
  const serviceType = service.agent_type as Record<string, unknown>;
  const llmId = String(serviceType.llm_ref_id ?? "");
  return llmRefName(llmId) || "未绑定";
}

function llmRefName(id: string): string {
  return llm.value.find((item) => item.config_id === id)?.name ?? "";
}

function runtimeBadgeText(service: ServiceWithRuntime): string {
  switch (service.runtime.status) {
    case "running":
      return service.runtime.instance_id
        ? `已启动 (${summarizeIds([service.runtime.instance_id])})`
        : "已启动";
    case "stopped":
      return "已停止";
    case "starting":
      return "启动中";
    case "error":
      return "启动失败";
    default:
      return service.runtime.status;
  }
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`Agent 页面加载失败: ${(error as Error).message}`);
  });
});

  return {
    serviceTypes,
    services,
    servicesLoading,
    connections,
    llm,
    workflows,
    form,
    editingServiceId,
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
    qqChatDefaultTools,
    httpStreamDefaultTools,
    workspaceDefaultTools,
    currentDefaultTools,
    defaultToolSearchQuery,
    filteredDefaultTools,
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
    mainChatModel,
    mainChatModelSupportsMultimodal,
    botConnections,
    rustfsConnections,
    webSearchEngineConnections,
    taskDbConnections,
    tokenizerConnections,
    imageWeaviateConnections,
    memoryWeaviateConnections,
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
    closeRateLimitModal,
    addGroupRateLimitRule,
    removeGroupRateLimitRule,
    addUserRateLimitRule,
    removeUserRateLimitRule,
    editIgnoreRule,
    submitIgnoreRule,
    removeIgnoreRule,
    addTool,
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
  };
}

export type UseAgentsReturn = ReturnType<typeof useAgents>;
