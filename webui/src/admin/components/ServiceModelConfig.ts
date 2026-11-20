import { computed } from "vue";

import type { ConnectionConfig, LlmConfig } from "../../api/client";
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

  return {
    chatModelOptions,
    multimodalChatModelOptions,
    tokenizerOptions,
  };
}
