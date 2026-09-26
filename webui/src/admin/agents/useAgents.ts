import { computed, markRaw, onMounted, reactive, ref } from "vue";
import { CodeIcon, FlowchartIcon, RobotIcon, TerminalIcon } from "tdesign-icons-vue-next";

import {
  system,
  workflows as workflowApi,
  type ServiceWithRuntime,
  type ConnectionConfig,
  type LlmConfig,
  type QqChatAgentServiceIgnoreRule,
  type WorkflowInfo,
  type SubAgentDefinition,
} from "../../api/client";
import {
  serviceFormFromConfig,
  buildServicePayload,
  WORKSPACE_DEFAULT_TOOLS,
  isBotAdapterConnectionType,
  QQ_CHAT_DEFAULT_TOOLS,
  defaultServiceForm,
  defaultQqChatDefaultToolsEnabled,
  defaultToolForm,
  defaultWorkspaceDefaultToolsEnabled,
  assertServiceConfig,
  compactId,
  formatTime,
  statusTone,
  summarizeIds,
  getAvatarDisplayUrl,
  agentAvatarUrl,
  agentInitial,
  subAgentReferenceableToolIds,
  formatDataType,
  type ServiceFormState,
  type ServiceTypeName,
  type ScriptLanguageForm,
  type QqChatEmotionDimensionFormItem,
} from "../model";
import { useAdminClipboard } from "../components/useAdminClipboard";

export function useAgents() {
type ServiceTypeOption = {
  value: ServiceTypeName;
  label: string;
  hint: string;
};

const serviceTypes: ServiceTypeOption[] = [
  {
    value: "qq_chat",
    label: "QQ Chat RoleService",
    hint: "通过 QQ Bot Adapter 提供对话服务",
  },
  {
    value: "workspace",
    label: "Workspace RoleService",
    hint: "面向项目目录的开发型 RoleService",
  },
];

const services = ref<ServiceWithRuntime[]>([]);
const servicesLoading = ref(false);
const connections = ref<ConnectionConfig[]>([]);
const llm = ref<LlmConfig[]>([]);
const workflows = ref<WorkflowInfo[]>([]);
const subAgents = ref<SubAgentDefinition[]>([]);
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
const { copiedId: serviceCopiedId, copyConfig: copyServiceConfig, handleFileChange: handleServiceFileChange } = useAdminClipboard<ServiceWithRuntime>({
  validate: assertServiceConfig,
  onImport: (config) => {
    Object.assign(form, serviceFormFromConfig(config));
    form.id = null;
    editingServiceId.value = "";
    form.name = `${form.name} 副本`;
    showCreatePicker.value = true;
    showCreateForm.value = true;
  },
  isEnabled: () => !showCreatePicker.value && !showEditModal.value,
});

const currentDefaultTools = computed(() => {
  if (form.type === "qq_chat") return QQ_CHAT_DEFAULT_TOOLS;
  if (form.type === "workspace") return WORKSPACE_DEFAULT_TOOLS;
  return [];
});

type ToolTableRow = {
  key: string;
  kind: "builtin" | "custom";
  label: string;
  id: string;
  description: string;
  toolIndex?: number;
  tool?: ServiceFormState["tools"][number];
};

const toolSearchQuery = ref("");

const toolRows = computed<ToolTableRow[]>(() => {
  const builtinRows: ToolTableRow[] = currentDefaultTools.value.map((tool) => ({
    key: `builtin:${tool.id}`,
    kind: "builtin",
    label: tool.label,
    id: tool.id,
    description: tool.description,
  }));
  const customRows: ToolTableRow[] = form.tools.map((tool, index) => ({
    key: `custom:${tool.id}:${index}`,
    kind: "custom",
    label: tool.name,
    id: tool.id,
    description: tool.description,
    toolIndex: index,
    tool,
  }));
  return [...builtinRows, ...customRows];
});

const filteredToolRows = computed(() => {
  const q = toolSearchQuery.value.trim().toLowerCase();
  if (!q) return toolRows.value;
  return toolRows.value.filter(
    (row) =>
      row.label.toLowerCase().includes(q) ||
      row.id.toLowerCase().includes(q) ||
      row.description.toLowerCase().includes(q),
  );
});

function toolRowClassName({ row }: { row: ToolTableRow }): string {
  if (row.kind === "custom") {
    return "tool-row--custom";
  }
  return form.default_tools_enabled[row.id] !== false
    ? "tool-row--builtin"
    : "tool-row--builtin-inactive";
}

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

const showToolEditModal = ref(false);
const toolCreateStep = ref<"picker" | "config">("config");
const editingToolIndex = ref(-1);
const toolEditDraft = reactive(defaultToolForm());
const toolEditCallLimit = ref<number | null>(null);
const toolEditOriginalName = ref("");

type ToolTypeOption = {
  /** The implementation this card creates. */
  value: ServiceFormState["tools"][number]["implementation"];
  /** Script tools share one implementation, so the card carries the language it presets. */
  scriptLanguage?: ScriptLanguageForm;
  label: string;
  desc: string;
  icon: unknown;
};

const toolTypeOptions: ToolTypeOption[] = [
  {
    value: "sub_agent",
    label: "Sub-agents",
    desc: "调用已定义的 Sub-agent",
    icon: markRaw(RobotIcon),
  },
  {
    value: "node_graph",
    label: "节点图",
    desc: "调用节点图 / Workflow Set",
    icon: markRaw(FlowchartIcon),
  },
  {
    value: "script",
    scriptLanguage: "typescript",
    label: "TypeScript 脚本",
    desc: "用 TypeScript 编写工具逻辑",
    icon: markRaw(CodeIcon),
  },
  {
    value: "script",
    scriptLanguage: "python",
    label: "Python 脚本",
    desc: "",
    icon: markRaw(TerminalIcon),
  },
];

/** The type picker only fronts tool creation; editing goes straight to the config form. */
const showToolTypeBack = computed(
  () => editingToolIndex.value === -1 && toolCreateStep.value === "config",
);

function openNewTool() {
  Object.assign(toolEditDraft, defaultToolForm());
  editingToolIndex.value = -1;
  toolEditCallLimit.value = null;
  toolEditOriginalName.value = "";
  resetScriptDraftState();
  toolCreateStep.value = "picker";
  showToolEditModal.value = true;
}

function selectToolType(option: ToolTypeOption) {
  Object.assign(toolEditDraft, defaultToolForm(), {
    implementation: option.value,
    scriptLanguage: option.scriptLanguage ?? toolEditDraft.scriptLanguage,
  });
  toolEditCallLimit.value = null;
  editingToolIndex.value = -1;
  resetScriptDraftState();
  toolCreateStep.value = "config";
}

function backToToolTypePicker() {
  Object.assign(toolEditDraft, defaultToolForm());
  toolEditCallLimit.value = null;
  resetScriptDraftState();
  toolCreateStep.value = "picker";
}

function selectSubAgent(id: string) {
  const definition = subAgents.value.find((item) => item.id === id);
  if (!definition) return;
  // The published sub-agent tool is named by its definition id, so the tool inherits it.
  toolEditDraft.id = definition.id;
  toolEditDraft.subAgentId = definition.id;
  toolEditDraft.name = definition.id;
  toolEditDraft.description = definition.description || definition.name;
  toolEditDraft.runDuration = definition.run_duration;
}

/** The definition behind `toolEditDraft.subAgentId`, previewed under the selector. */
const selectedSubAgent = computed(() =>
  subAgents.value.find((item) => item.id === toolEditDraft.subAgentId),
);

const subAgentOutputModeLabel = computed(() =>
  selectedSubAgent.value?.output_mode === "text" ? "纯文本" : "JSON 输出端口",
);

function openToolEdit(index: number) {
  const tool = form.tools[index];
  if (!tool) return;
  Object.assign(toolEditDraft, tool);
  editingToolIndex.value = index;
  toolEditCallLimit.value = form.tool_session_call_limits[tool.name] ?? null;
  toolEditOriginalName.value = tool.name;
  // An existing tool already has its entry; inference would only fight the saved value.
  resetScriptDraftState();
  scriptEntryManuallyEdited.value = true;
  toolCreateStep.value = "config";
  showToolEditModal.value = true;
}

function closeToolEditModal() {
  showToolEditModal.value = false;
}

function confirmToolEdit() {
  if (!validateToolDraft()) {
    return;
  }
  const draft = { ...toolEditDraft };
  const previousName = editingToolIndex.value === -1 ? "" : toolEditOriginalName.value;
  if (editingToolIndex.value === -1) {
    form.tools.push(draft);
  } else {
    const existing = form.tools[editingToolIndex.value];
    if (existing) {
      Object.assign(existing, draft);
    }
  }
  if (previousName && previousName !== draft.name) {
    delete form.tool_session_call_limits[previousName];
  }
  if (draft.name && toolEditCallLimit.value != null && toolEditCallLimit.value > 0) {
    form.tool_session_call_limits[draft.name] = toolEditCallLimit.value;
  } else if (draft.name) {
    delete form.tool_session_call_limits[draft.name];
  }
  showToolEditModal.value = false;
}

function validateToolDraft(): boolean {
  if (toolEditDraft.implementation === "sub_agent") {
    if (!toolEditDraft.subAgentId) {
      alert("请选择 Sub-agent");
      return false;
    }
    const duplicated = form.tools.some(
      (tool, index) =>
        tool.implementation === "sub_agent" &&
        tool.subAgentId === toolEditDraft.subAgentId &&
        index !== editingToolIndex.value,
    );
    if (duplicated) {
      alert(`Sub-agent '${toolEditDraft.subAgentId}' 已经添加过了`);
      return false;
    }
    return true;
  }

  if (!toolEditDraft.name.trim()) {
    alert("请填写工具名称");
    return false;
  }
  if (toolEditDraft.implementation === "script") {
    if (!toolEditDraft.scriptSource.trim()) {
      alert("请填写脚本内容或上传脚本文件");
      return false;
    }
    if (!toolEditDraft.scriptEntry.trim()) {
      alert("请填写入口函数");
      return false;
    }
    if (!Number.isFinite(toolEditDraft.scriptTimeoutSecs) || toolEditDraft.scriptTimeoutSecs < 1) {
      alert("超时时间必须大于 0 秒");
      return false;
    }
    if (!isJsonArray(toolEditDraft.parametersJson)) {
      alert("Parameters JSON 不是合法的 JSON 数组");
      return false;
    }
    if (!isJsonArray(toolEditDraft.outputsJson)) {
      alert("Outputs JSON 不是合法的 JSON 数组");
      return false;
    }
    // The runtime enforces the output signature when the service starts; checking it here keeps
    // a mismatch in the editor instead of surfacing it as a start failure.
    const outputs = JSON.parse(toolEditDraft.outputsJson || "[]") as Array<{
      name?: unknown;
      data_type?: unknown;
    }>;
    if (form.type === "qq_chat") {
      // qq_chat hands tool results to the model as plain text, so exactly one String output is
      // allowed and the script returns that string directly.
      if (outputs.length !== 1) {
        alert("qq_chat Service 的脚本工具必须声明且只声明一个输出");
        return false;
      }
      const dataType = outputs[0]?.data_type;
      if (dataType !== "String") {
        alert("qq_chat Service 的脚本工具输出必须是 String 类型");
        return false;
      }
    } else if (outputs.length === 0) {
      alert("脚本工具至少需要一个输出");
      return false;
    }
    const scriptNameTaken = form.tools.some(
      (tool, index) => tool.name === toolEditDraft.name.trim() && index !== editingToolIndex.value,
    );
    if (scriptNameTaken) {
      alert(`工具名称 '${toolEditDraft.name.trim()}' 已经存在`);
      return false;
    }
    return true;
  }

  if (toolEditDraft.targetType === "workflow_set" && !toolEditDraft.workflowName) {
    alert("请选择节点图");
    return false;
  }
  if (toolEditDraft.targetType === "file_path" && !toolEditDraft.filePath.trim()) {
    alert("请填写文件路径");
    return false;
  }
  if (toolEditDraft.targetType === "inline_graph" && !isJsonObject(toolEditDraft.inlineGraphJson)) {
    alert("Inline Graph JSON 不是合法的 JSON 对象");
    return false;
  }
  if (!isJsonArray(toolEditDraft.parametersJson)) {
    alert("Parameters JSON 不是合法的 JSON 数组");
    return false;
  }
  if (!isJsonArray(toolEditDraft.outputsJson)) {
    alert("Outputs JSON 不是合法的 JSON 数组");
    return false;
  }
  const nameTaken = form.tools.some(
    (tool, index) => tool.name === toolEditDraft.name.trim() && index !== editingToolIndex.value,
  );
  if (nameTaken) {
    alert(`工具名称 '${toolEditDraft.name.trim()}' 已经存在`);
    return false;
  }
  return true;
}

function isJsonArray(raw: string): boolean {
  try {
    return Array.isArray(JSON.parse(raw || "[]"));
  } catch {
    return false;
  }
}

function isJsonObject(raw: string): boolean {
  try {
    const parsed: unknown = JSON.parse(raw || "{}");
    return Boolean(parsed) && typeof parsed === "object" && !Array.isArray(parsed);
  } catch {
    return false;
  }
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
const retrievalConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "weaviate" || item.kind.type === "elasticsearch"),
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
    const [loadedAgents, loadedConnections, loadedLlm, loadedWorkflows, loadedSubAgents] =
      await Promise.all([
        system.services.list(),
        system.connections.list(),
        system.llm.list(),
        workflowApi.listDetailed(),
        system.subagents.list(subAgentReferenceableToolIds()),
      ]);
    services.value = loadedAgents;
    connections.value = loadedConnections;
    llm.value = loadedLlm;
    workflows.value = loadedWorkflows.workflows;
    subAgents.value = loadedSubAgents;
  } finally {
    servicesLoading.value = false;
  }
}

function editService(service: ServiceWithRuntime) {
  Object.assign(form, serviceFormFromConfig(service));
  editingServiceId.value = service.config_id;
  showEditModal.value = true;
}

function duplicateService(service: ServiceWithRuntime) {
  Object.assign(form, serviceFormFromConfig(service));
  form.id = null;
  editingServiceId.value = "";
  form.name = `${form.name} 副本`;
  showCreatePicker.value = true;
  showCreateForm.value = true;
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

function removeTool(index: number) {
  form.tools.splice(index, 1);
}

function validateImageUnderstandModelSelection(): string | null {
  if (!form.default_tools_enabled.image_understand) {
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

/** Extensions the file picker accepts for one script language. */
const SCRIPT_FILE_EXTENSIONS: Record<ScriptLanguageForm, string[]> = {
  typescript: [".ts", ".mts", ".mjs"],
  python: [".py"],
};

/** Mirrors the backend's stored-script cap so an oversized file is rejected before upload. */
const MAX_SCRIPT_SOURCE_BYTES = 256 * 1024;

const scriptUploadInput = ref<HTMLInputElement | null>(null);

function scriptFileAccept(language: ScriptLanguageForm): string {
  return SCRIPT_FILE_EXTENSIONS[language].join(",");
}

/** The file name last read into the editor, shown so an upload is visibly acknowledged. */
const scriptSourceFileName = ref("");

function openScriptFilePicker() {
  scriptUploadInput.value?.click();
}

async function handleScriptFileSelected(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) {
    return;
  }
  const extensions = SCRIPT_FILE_EXTENSIONS[toolEditDraft.scriptLanguage];
  if (!extensions.some((extension) => file.name.toLowerCase().endsWith(extension))) {
    alert(`请选择 ${extensions.join(" / ")} 文件`);
    return;
  }
  if (file.size > MAX_SCRIPT_SOURCE_BYTES) {
    alert(`脚本文件不能超过 ${MAX_SCRIPT_SOURCE_BYTES / 1024} KiB`);
    return;
  }
  toolEditDraft.scriptSource = await file.text();
  scriptSourceFileName.value = file.name;
  inferScriptEntryIfUntouched();
  if (!toolEditDraft.name.trim()) {
    toolEditDraft.name = file.name.replace(/\.[^.]+$/, "");
  }
}

const syncingScriptManifest = ref(false);

/** Skeleton shown in the empty script editor, matching the selected language's syntax. */
const scriptEditorPlaceholder = computed(() =>
  toolEditDraft.scriptLanguage === "python"
    ? 'def run_tool(request, zihuan):\n    text = request["arguments"].get("text", "")\n    return {"ok": True, "result": {"result": f"echo: {text}"}}'
    : 'export async function run_tool(request, zihuan) {\n  const text = request.arguments?.text ?? "";\n  return { ok: true, result: { result: `echo: ${text}` } };\n}',
);

/** True once the user edits the entry field in this dialog; inference stops for good after that. */
const scriptEntryManuallyEdited = ref(false);

/** Clears the per-draft script state, so a new tool starts inferring its entry again. */
function resetScriptDraftState() {
  scriptEntryManuallyEdited.value = false;
  scriptSourceFileName.value = "";
}

/**
 * Guesses a script's entry function name from its source.
 *
 * Returns null when the source declares nothing recognizable, so a half-written script never
 * clears the field. A Python script is read for its first module-level `def`, a TypeScript one
 * for an exported function, then for an exported arrow function.
 */
function inferScriptEntry(language: ScriptLanguageForm, source: string): string | null {
  if (language === "python") {
    for (const line of source.split(/\r?\n/)) {
      const match = /^def\s+([A-Za-z_]\w*)\s*\(/.exec(line);
      if (match && !match[1].startsWith("_")) {
        return match[1];
      }
    }
    return null;
  }
  const declared = /^\s*export\s+(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)/m.exec(source);
  if (declared) {
    return declared[1];
  }
  const bound = /^\s*export\s+const\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\(?[^)=\n]*\)?\s*=>/m.exec(source);
  return bound ? bound[1] : null;
}

/** Fills the entry field from the script, unless the user has taken that field over. */
function inferScriptEntryIfUntouched() {
  if (scriptEntryManuallyEdited.value) {
    return;
  }
  const inferred = inferScriptEntry(toolEditDraft.scriptLanguage, toolEditDraft.scriptSource);
  if (inferred) {
    toolEditDraft.scriptEntry = inferred;
  }
}

/** Called as the script source changes, so the entry field tracks the code being written. */
function handleScriptSourceEdited() {
  inferScriptEntryIfUntouched();
}

/** Called when the language changes: the same source is read by a different parser. */
function handleScriptLanguageChanged() {
  inferScriptEntryIfUntouched();
}

/** Called when the user touches the entry field, which ends inference for this draft. */
function handleScriptEntryEdited() {
  scriptEntryManuallyEdited.value = true;
}

/**
 * Fills the parameters and outputs from the script's own manifest export.
 *
 * Reading the manifest is what lets a script describe its LLM-facing signature in code; a script
 * without one keeps whatever the form already holds.
 */
async function syncToolManifestFromScript() {
  if (!toolEditDraft.scriptSource.trim()) {
    alert("请先填写脚本内容或上传脚本文件");
    return;
  }
  syncingScriptManifest.value = true;
  try {
    const manifest = await system.scriptTools.manifest({
      language: toolEditDraft.scriptLanguage,
      source: toolEditDraft.scriptSource,
      entry: toolEditDraft.scriptEntry.trim() || "run_tool",
    });
    if (manifest.parameters.length === 0 && manifest.outputs.length === 0) {
      alert("脚本未声明 TOOL_MANIFEST / tool_manifest，请手动填写 Parameters 与 Outputs");
      return;
    }
    toolEditDraft.parametersJson = JSON.stringify(manifest.parameters, null, 2);
    toolEditDraft.outputsJson = JSON.stringify(manifest.outputs, null, 2);
  } catch (error) {
    alert(error instanceof Error ? error.message : String(error));
  } finally {
    syncingScriptManifest.value = false;
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

  // The tool id is system-assigned: a workflow-set tool is identified by the workflow it calls,
  // which also keeps the id unique across the service's tool list.
  tool.id = workflow.name;
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
    if (form.type === "workspace" && form.workspace_memory_enabled) {
      if (!form.workspace_memory_backend) {
        alert("启用 Agent 记忆后必须选择记忆库");
        return;
      }
      if (
        form.workspace_memory_backend === "retrieval_store" &&
        (!form.workspace_retrieval_store_id || !form.workspace_embedding_model_ref_id)
      ) {
        alert("检索数据库记忆库需要选择记忆库连接和文本向量模型");
        return;
      }
    }
    if (form.type === "workspace" && form.default_tools_enabled.web_search && !form.web_search_engine_connection_id) {
      alert("启用联网搜索后必须选择 Web Search Engine 连接");
      return;
    }
    if (form.type === "qq_chat" && !form.ims_bot_adapter_connection_id) {
      alert("QQ Chat RoleService 需要绑定 Bot Adapter");
      return;
    }
    if (form.type === "qq_chat" && !form.web_search_engine_connection_id) {
      alert("QQ Chat RoleService 需要绑定 Web Search Engine 连接");
      return;
    }
    const imageUnderstandError = validateImageUnderstandModelSelection();
    if (imageUnderstandError) {
      alert(imageUnderstandError);
      return;
    }
    if (
      form.type === "qq_chat" &&
      form.retrieval_store_id &&
      form.retrieval_store_id !== "__local_markdown__" &&
      !form.embedding_model_ref_id
    ) {
      alert("QQ Chat RoleService 启用记忆库时需要绑定文本向量模型");
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
  const serviceType = service.role_service_type as Record<string, unknown>;
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
    currentDefaultTools,
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
    mainChatModel,
    mainChatModelSupportsMultimodal,
    botConnections,
    rustfsConnections,
    webSearchEngineConnections,
    taskDbConnections,
    tokenizerConnections,
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
    toolCreateStep,
    toolTypeOptions,
    showToolTypeBack,
    subAgents,
    editingToolIndex,
    toolEditDraft,
    toolEditCallLimit,
    openNewTool,
    selectToolType,
    backToToolTypePicker,
    selectSubAgent,
    selectedSubAgent,
    subAgentOutputModeLabel,
    openToolEdit,
    closeToolEditModal,
    confirmToolEdit,
    validateToolDraft,
    removeTool,
    validateImageUnderstandModelSelection,
    syncingToolIndex,
    syncToolFromGraph,
    handleToolTargetTypeChange,
    applyWorkflowSetMetadata,
    scriptUploadInput,
    scriptSourceFileName,
    scriptFileAccept,
    openScriptFilePicker,
    handleScriptFileSelected,
    syncingScriptManifest,
    syncToolManifestFromScript,
    scriptEditorPlaceholder,
    scriptEntryManuallyEdited,
    handleScriptSourceEdited,
    handleScriptLanguageChanged,
    handleScriptEntryEdited,
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
    formatDataType,
    serviceCopiedId,
    copyServiceConfig,
    handleServiceFileChange,
  };
}

export type UseAgentsReturn = ReturnType<typeof useAgents>;
