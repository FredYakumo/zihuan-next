import { computed, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import {
  fileIO,
  system,
  isChatModelRef,
  type LlmConfig,
  type LocalLlmModelInfo,
} from "../../api/client";
import {
  assertLlmConfig,
  buildModelRefPayload,
  compactId,
  defaultLlmForm,
  formatTime,
  llmFormFromConfig,
  type LlmFormState,
} from "../model";
import { useAdminClipboard } from "../components/useAdminClipboard";

type LlmFilters = {
  keyword: string;
  modelType: "chat_llm" | "text_embedding" | "all";
  enabled: "all" | "enabled" | "disabled";
};

function isEmbeddingType(type: string): boolean {
  return type === "text_embedding_local" || type === "text_embedding_api";
}

export function useLlm() {
  const items = ref<LlmConfig[]>([]);
  const form = reactive<LlmFormState>(defaultLlmForm());
  const drawerVisible = ref(false);
  const filters = reactive<LlmFilters>({
    keyword: "",
    modelType: "all",
    enabled: "all",
  });
  const localEmbeddingModels = ref<string[]>([]);
  const localLlmModels = ref<LocalLlmModelInfo[]>([]);
  const downloadDialogVisible = ref(false);
  const updatingEnabledIds = ref(new Set<string>());
  const isCreating = computed(() => form.id === null);
  const filteredItems = computed(() => {
    const keyword = filters.keyword.trim().toLowerCase();
    return items.value.filter((item) => {
      if (filters.modelType === "chat_llm" && !isChatModelRef(item.model)) {
        return false;
      }
      if (filters.modelType === "text_embedding" && !isEmbeddingType(item.model.type)) {
        return false;
      }
      if (filters.enabled === "enabled" && !item.enabled) {
        return false;
      }
      if (filters.enabled === "disabled" && item.enabled) {
        return false;
      }
      if (!keyword) {
        return true;
      }
      const modelName = isChatModelRef(item.model) ? item.model.llm.model_name : item.model.model_name;
      return [item.name, item.config_id, modelName].join(" ").toLowerCase().includes(keyword);
    });
  });

  const { copiedId, copyConfig, handleFileChange } = useAdminClipboard<LlmConfig>({
    validate: assertLlmConfig,
    onImport: (config) => {
      Object.assign(form, llmFormFromConfig(config));
      form.id = null;
      form.name = `${form.name} 副本`;
      drawerVisible.value = true;
    },
    isEnabled: () => !drawerVisible.value,
  });

  const isChatMode = computed(
    () => form.model_type === "chat_llm_local" || form.model_type === "chat_llm_api",
  );
  /** 本地 GGUF/HF 权重推理（Mock 是录制回放，不走本地权重目录）。 */
  const isWeightMode = computed(
    () => form.model_type === "chat_llm_local" && form.llm.weight_style !== "mock",
  );
  /** 本地模型 / API 接入方式：聊天模型映射到 model_type，向量模型也映射到 model_type。 */
  const accessMode = computed<"local" | "api">({
    get: () => {
      if (isChatMode.value) {
        return form.model_type === "chat_llm_local" ? "local" : "api";
      }
      return form.model_type === "text_embedding_api" ? "api" : "local";
    },
    set: (mode) => {
      if (isChatMode.value) {
        if (mode === "local") {
          form.model_type = "chat_llm_local";
          if (!form.llm.weight_style) {
            form.llm.weight_style = "gguf";
          }
        } else {
          form.model_type = "chat_llm_api";
        }
        return;
      }
      form.model_type = mode === "api" ? "text_embedding_api" : "text_embedding_local";
    },
  });
  /** 模型类型下拉：两种聊天模型 / 两种向量模型各合并为一项。 */
  const modelTypeSelect = computed<"chat_llm" | "text_embedding">({
    get: () => (isChatMode.value ? "chat_llm" : "text_embedding"),
    set: (value) => {
      if (value === "chat_llm") {
        if (!isChatMode.value) {
          form.model_type = "chat_llm_api";
        }
      } else if (isChatMode.value) {
        form.model_type = "text_embedding_local";
      }
    },
  });
  const selectedLocalLlm = computed(
    () =>
      localLlmModels.value.find((item) => item.model_name === form.llm.model_name) ?? null,
  );
  const filteredLocalLlmModels = computed(() => {
    if (!isWeightMode.value) {
      return localLlmModels.value;
    }
    const expectedLayout = form.llm.weight_style === "hf" ? "hf" : "gguf";
    return localLlmModels.value.filter((item) => item.layout === expectedLayout);
  });
  const selectedLocalLlmHint = computed(() => {
    if (form.model_type !== "chat_llm_local") {
      return "";
    }
    if (form.llm.weight_style === "mock") {
      return "本地模拟模式会回放 api_endpoint 指向目录中的调用记录。";
    }
    if (!selectedLocalLlm.value) {
      return "请选择一个本地模型目录。";
    }
    if (!selectedLocalLlm.value.available) {
      return selectedLocalLlm.value.reason ?? "该模型目录当前不可用。";
    }
    return `类型：${selectedLocalLlm.value.kind}；格式：${selectedLocalLlm.value.layout}；${
      selectedLocalLlm.value.supports_multimodal_input ? "支持图片多模态" : "文本模型"
    }`;
  });

  function resetCreateForm() {
    Object.assign(form, defaultLlmForm());
  }

  const route = useRoute();
  const router = useRouter();
  const importFileInput = ref<HTMLInputElement | null>(null);
  const columns = [
    { colKey: "name", title: "配置名称", width: 210 },
    { colKey: "model_type", title: "模型类型", width: 150 },
    { colKey: "model", title: "模型", ellipsis: true },
    { colKey: "endpoint", title: "接口地址", ellipsis: true },
    { colKey: "updated_at", title: "更新时间", width: 170 },
    { colKey: "actions", title: "操作", width: 340, fixed: "right" as const },
  ];
  const contextLengthPresets = [
    { label: "64K", value: 64 * 1024 },
    { label: "256K", value: 256 * 1024 },
    { label: "1M", value: 1024 * 1024 },
  ];

  function triggerImportFile() {
    importFileInput.value?.click();
  }

  function modelTypeLabel(type: string) {
    return type === "chat_llm_local" || type === "chat_llm_api" ? "聊天模型" : "文本向量模型";
  }

  function modelName(item: LlmConfig) {
    return isChatModelRef(item.model) ? item.model.llm.model_name : item.model.model_name;
  }

  function apiStyleLabel(apiStyle: string) {
    const labels: Record<string, string> = {
      open_ai_chat_completions: "OpenAI Chat Completions",
      open_ai_chat_completions_tencent_multimodal_compat: "腾讯多模态兼容",
      open_ai_responses: "OpenAI Responses",
      open_ai_responses_message_compat: "Responses message 兼容",
      open_ai_responses_image_url_object_compat: "Responses image_url 兼容",
    };
    return labels[apiStyle] ?? apiStyle;
  }

  function weightStyleLabel(weightStyle: string | null | undefined) {
    const labels: Record<string, string> = {
      mock: "本地模拟",
      gguf: "GGUF",
      hf: "HF",
    };
    return weightStyle ? labels[weightStyle] ?? weightStyle : "本地模型";
  }

  function copyLlmConfig(item: LlmConfig) {
    copyConfig({ name: item.name, enabled: item.enabled, model: item.model }, item.config_id);
  }

  if (route.query.action === "create") startCreate();

  function resetForm() {
    resetCreateForm();
  }

  function startCreate() {
    resetCreateForm();
    drawerVisible.value = true;
  }

  function closeDrawer() {
    resetCreateForm();
    drawerVisible.value = false;
  }

  async function load() {
    const [models, localModels, localLlmModelList] = await Promise.all([
      system.llm.list(),
      fileIO.listTextEmbeddingModels(),
      fileIO.listLocalLlmModels(),
    ]);
    items.value = models;
    localEmbeddingModels.value = localModels.models;
    localLlmModels.value = localLlmModelList.models;
  }

  async function reloadLocalEmbeddingModels(): Promise<string[]> {
    const localModels = await fileIO.listTextEmbeddingModels();
    localEmbeddingModels.value = localModels.models;
    return localModels.models;
  }

  async function onEmbeddingDownloadCompleted(modelName: string) {
    const models = await reloadLocalEmbeddingModels();
    if (form.model_type === "text_embedding_local" && models.includes(modelName)) {
      form.local_model_name = modelName;
    }
  }

  function editItem(item: LlmConfig) {
    Object.assign(form, llmFormFromConfig(item));
    drawerVisible.value = true;
  }

  async function submitForm() {
    if (!form.name.trim()) {
      alert("请至少填写名称");
      return;
    }
    if (isChatMode.value) {
      if (!Number.isInteger(form.llm.context_length) || form.llm.context_length <= 0) {
        alert("上下文长度必须为大于 0 的整数");
        return;
      }
      if (form.model_type === "chat_llm_local") {
        if (!form.llm.weight_style) {
          alert("请选择本地运行方式");
          return;
        }
        if (form.llm.weight_style === "mock") {
          if (!form.llm.model_name.trim() || !form.llm.api_endpoint.trim()) {
            alert("请填写模型名和录制目录");
            return;
          }
        } else {
          if (!form.llm.model_name.trim()) {
            alert("请选择本地模型目录");
            return;
          }
          if (!selectedLocalLlm.value?.available) {
            alert(selectedLocalLlm.value?.reason ?? "所选本地模型当前不可用");
            return;
          }
        }
      } else if (!form.llm.model_name.trim() || !form.llm.api_endpoint.trim()) {
        alert("请至少填写名称、模型名和 API Endpoint");
        return;
      }
    } else if (form.model_type === "text_embedding_api") {
      if (!form.embedding.model_name.trim() || !form.embedding.api_endpoint.trim()) {
        alert("请填写模型名和 API Endpoint");
        return;
      }
    } else if (!form.local_model_name.trim()) {
      alert("请选择本地文本向量模型目录");
      return;
    }
    const payload = {
      name: form.name.trim(),
      enabled: form.enabled,
      model: buildModelRefPayload(form),
    };
    if (form.id) {
      await system.llm.update(form.id, payload);
    } else {
      await system.llm.create(payload);
    }
    resetCreateForm();
    drawerVisible.value = false;
    await load();
  }

  async function removeItem(id: string) {
    await system.llm.delete(id);
    if (form.id === id) {
      closeDrawer();
    }
    await load();
  }

  async function updateEnabled(item: LlmConfig, enabled: boolean) {
    if (item.enabled === enabled || updatingEnabledIds.value.has(item.config_id)) {
      return;
    }

    updatingEnabledIds.value.add(item.config_id);
    try {
      const updatedItem = await system.llm.update(item.config_id, {
        name: item.name,
        enabled,
        model: item.model,
      });
      Object.assign(item, updatedItem);
    } catch (error) {
      console.error(error);
      alert(`更新模型启用状态失败: ${(error as Error).message}`);
    } finally {
      updatingEnabledIds.value.delete(item.config_id);
    }
  }

  onMounted(() => {
    load().catch((error) => {
      console.error(error);
      alert(`模型配置加载失败: ${(error as Error).message}`);
    });
  });

  watch(
    () => [form.model_type, form.llm.weight_style, form.llm.model_name],
    () => {
      if (form.model_type !== "chat_llm_local" || form.llm.weight_style === "mock") {
        return;
      }
      form.llm.api_endpoint = "";
      form.llm.api_key = "";
      form.llm.supports_multimodal_input = Boolean(
        selectedLocalLlm.value?.supports_multimodal_input,
      );
    },
    { immediate: true },
  );

  function localLlmOptionLabel(item: LocalLlmModelInfo): string {
    const tags = [item.kind, item.layout];
    const suffix = item.available ? "" : ` - 不可用: ${item.reason ?? "未知原因"}`;
    return `${item.model_name} [${tags.join("/")}]${suffix}`;
  }

  watch(
    () => form.llm.weight_style,
    (weightStyle) => {
      if (form.model_type !== "chat_llm_local" || !weightStyle || weightStyle === "mock") {
        return;
      }
      const expectedLayout = weightStyle === "hf" ? "hf" : "gguf";
      if (selectedLocalLlm.value?.layout !== expectedLayout) {
        form.llm.model_name = "";
      }
    },
  );

  return {
    items,
    filteredItems,
    form,
    drawerVisible,
    isCreating,
    filters,
    localEmbeddingModels,
    localLlmModels,
    downloadDialogVisible,
    onEmbeddingDownloadCompleted,
    isChatMode,
    isWeightMode,
    accessMode,
    modelTypeSelect,
    selectedLocalLlm,
    filteredLocalLlmModels,
    selectedLocalLlmHint,
    resetCreateForm,
    resetForm,
    startCreate,
    closeDrawer,
    load,
    editItem,
    submitForm,
    removeItem,
    updateEnabled,
    updatingEnabledIds,
    localLlmOptionLabel,
    compactId,
    formatTime,
    copiedId,
    copyConfig,
    handleFileChange,
    route,
    router,
    importFileInput,
    columns,
    contextLengthPresets,
    triggerImportFile,
    modelTypeLabel,
    modelName,
    weightStyleLabel,
    apiStyleLabel,
    copyLlmConfig,
  };
}

export type UseLlmReturn = ReturnType<typeof useLlm>;
