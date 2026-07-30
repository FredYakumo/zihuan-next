import { computed, onMounted, reactive, ref, watch } from "vue";

import { fileIO, system, type LlmConfig, type LocalLlmModelInfo } from "../../api/client";
import {
  assertLlmConfig,
  buildModelRefPayload,
  compactId,
  defaultLlmForm,
  formatTime,
  llmFormFromConfig,
  type LlmFormState,
} from "../model";
import { useAdminClipboard } from "./useAdminClipboard";

type LlmFilters = {
  keyword: string;
  modelType: LlmFormState["model_type"] | "all";
  enabled: "all" | "enabled" | "disabled";
};

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
  const updatingEnabledIds = ref(new Set<string>());
  const isCreating = computed(() => form.id === null);
  const filteredItems = computed(() => {
    const keyword = filters.keyword.trim().toLowerCase();
    return items.value.filter((item) => {
      if (filters.modelType !== "all" && item.model.type !== filters.modelType) {
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
      const modelName = item.model.type === "chat_llm" ? item.model.llm.model_name : item.model.model_name;
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

  const isCandleMode = computed(
    () =>
      form.model_type === "chat_llm" &&
      (form.llm.api_style === "candle_gguf" || form.llm.api_style === "candle_hf"),
  );
  const selectedLocalLlm = computed(
    () =>
      localLlmModels.value.find((item) => item.model_name === form.llm.model_name) ?? null,
  );
  const filteredLocalLlmModels = computed(() => {
    if (!isCandleMode.value) {
      return localLlmModels.value;
    }
    const expectedLayout = form.llm.api_style === "candle_gguf" ? "gguf" : "hf";
    return localLlmModels.value.filter((item) => item.layout === expectedLayout);
  });
  const selectedLocalLlmHint = computed(() => {
    if (!isCandleMode.value) {
      return "Candle 模式会从 models/llm 自动扫描可选目录。";
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

  function editItem(item: LlmConfig) {
    Object.assign(form, llmFormFromConfig(item));
    drawerVisible.value = true;
  }

  async function submitForm() {
    if (!form.name.trim()) {
      alert("请至少填写名称");
      return;
    }
    if (form.model_type === "chat_llm") {
      if (isCandleMode.value) {
        if (!form.llm.model_name.trim()) {
          alert("请选择本地 Candle 模型目录");
          return;
        }
        if (!selectedLocalLlm.value?.available) {
          alert(selectedLocalLlm.value?.reason ?? "所选本地模型当前不可用");
          return;
        }
      } else if (!form.llm.model_name.trim() || !form.llm.api_endpoint.trim()) {
        alert("请至少填写名称、模型名和 API Endpoint");
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
    () => [form.model_type, form.llm.api_style, form.llm.model_name],
    () => {
      if (!isCandleMode.value) {
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
    () => form.llm.api_style,
    (apiStyle) => {
      if (!isCandleMode.value) {
        return;
      }
      const expectedLayout = apiStyle === "candle_gguf" ? "gguf" : "hf";
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
    isCandleMode,
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
  };
}

export type UseLlmReturn = ReturnType<typeof useLlm>;
