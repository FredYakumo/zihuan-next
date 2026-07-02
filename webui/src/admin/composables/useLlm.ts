import { computed, onMounted, reactive, ref, watch } from "vue";

import { fileIO, system, type LlmConfig, type LocalLlmModelInfo } from "../../api/client";
import {
  buildModelRefPayload,
  compactId,
  defaultLlmForm,
  formatTime,
  llmFormFromConfig,
  type LlmFormState,
} from "../model";


export function useLlm() {
  const items = ref<LlmConfig[]>([]);
  const form = reactive<LlmFormState>(defaultLlmForm());
  const showCreatePicker = ref(false);
  const showCreateForm = ref(false);
  const localEmbeddingModels = ref<string[]>([]);
  const localLlmModels = ref<LocalLlmModelInfo[]>([]);

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
    showCreatePicker.value = true;
    showCreateForm.value = false;
  }

  function closeCreatePicker() {
    resetCreateForm();
    showCreatePicker.value = false;
    showCreateForm.value = false;
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
    showCreatePicker.value = false;
    showCreateForm.value = false;
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
    closeCreatePicker();
    await load();
  }

  async function removeItem(id: string) {
    if (!window.confirm("确认删除这个模型配置吗？")) {
      return;
    }
    await system.llm.delete(id);
    if (form.id === id) {
      resetCreateForm();
    }
    await load();
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
    form,
    showCreatePicker,
    showCreateForm,
    localEmbeddingModels,
    localLlmModels,
    isCandleMode,
    selectedLocalLlm,
    filteredLocalLlmModels,
    selectedLocalLlmHint,
    resetCreateForm,
    resetForm,
    startCreate,
    closeCreatePicker,
    load,
    editItem,
    submitForm,
    removeItem,
    localLlmOptionLabel,
    compactId,
    formatTime,
  };
}

export type UseLlmReturn = ReturnType<typeof useLlm>;
