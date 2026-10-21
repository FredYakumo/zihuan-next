import { computed, ref } from "vue";

import {
  getMemoryAgentPromptsSettings,
  request,
  type ConnectionConfig,
  type LlmConfig,
  type MemoryAgentPromptsSettings,
} from "../../api/client";
import type { ServiceFormState } from "../model";

export interface ServiceModelConfigProps {
  form: ServiceFormState;
  chatModels: LlmConfig[];
  multimodalChatModels: LlmConfig[];
  tokenizerConnections: ConnectionConfig[];
}

export interface ServiceModelConfigEmit {
  (event: "primary-model-change", value: string | number): void;
  (event: "image-understand-model-change", value: string | number): void;
  (event: "memory-backend-change", value: string | number): void;
}

export function useServiceModelConfig(
  props: ServiceModelConfigProps,
  _emit: ServiceModelConfigEmit,
) {
  const chatModelOptions = computed(() =>
    props.chatModels.map((model) => ({ value: model.config_id, label: model.name })),
  );
  const multimodalChatModelOptions = computed(() =>
    props.multimodalChatModels.map((model) => ({ value: model.config_id, label: model.name })),
  );
  const tokenizerOptions = computed(() =>
    props.tokenizerConnections.map((connection) => ({
      value: connection.config_id,
      label: connection.name,
    })),
  );

  const memoryPromptsVisible = ref(false);
  const memoryPromptsSaving = ref(false);
  const memoryPromptsError = ref("");
  const memoryPrompts = ref<MemoryAgentPromptsSettings>({
    system_prompt: "",
    search_operation_prompt: "",
    update_operation_prompt: "",
    defaults: { system_prompt: "", search_operation_prompt: "", update_operation_prompt: "" },
  });
  const GENERIC_MEMORY_PROMPT_PLACEHOLDERS = {
    system_prompt: "记忆代理的系统提示词",
    search_operation_prompt: "调用方强制搜索记忆时追加的提示词",
    update_operation_prompt: "调用方强制写入记忆时追加的提示词",
  };
  const memoryPromptDefaults = ref({ ...GENERIC_MEMORY_PROMPT_PLACEHOLDERS });

  async function openMemoryPromptsDialog() {
    memoryPromptsError.value = "";
    try {
      memoryPrompts.value = await getMemoryAgentPromptsSettings();
      memoryPromptDefaults.value = memoryPrompts.value.defaults ?? {
        ...GENERIC_MEMORY_PROMPT_PLACEHOLDERS,
      };
    } catch (cause) {
      memoryPromptsError.value = cause instanceof Error ? cause.message : String(cause);
    }
    memoryPromptsVisible.value = true;
  }

  async function saveMemoryPrompts() {
    memoryPromptsError.value = "";
    memoryPromptsSaving.value = true;
    try {
      memoryPrompts.value = await request<MemoryAgentPromptsSettings>(
        "PUT",
        "/settings/memory-agent-settings",
        {
          system_prompt: memoryPrompts.value.system_prompt,
          search_operation_prompt: memoryPrompts.value.search_operation_prompt,
          update_operation_prompt: memoryPrompts.value.update_operation_prompt,
        }
      );
      memoryPromptDefaults.value = memoryPrompts.value.defaults;
      memoryPromptsVisible.value = false;
    } catch (cause) {
      memoryPromptsError.value = cause instanceof Error ? cause.message : String(cause);
    } finally {
      memoryPromptsSaving.value = false;
    }
  }

  return {
    chatModelOptions,
    multimodalChatModelOptions,
    tokenizerOptions,
    memoryPromptsVisible,
    memoryPromptsSaving,
    memoryPromptsError,
    memoryPrompts,
    memoryPromptDefaults,
    openMemoryPromptsDialog,
    saveMemoryPrompts,
  };
}
