import { computed, type Ref } from "vue";
import type { LlmSetupConfig } from "../../api/client";

export function useLlmConfigStep(model: Ref<LlmSetupConfig>) {
  const canProceed = computed(() => {
    if (model.value.mode === "remote") {
      return (
        (model.value.model_name.trim().length > 0 ||
          (model.value.model_id?.trim().length ?? 0) > 0) &&
        model.value.api_endpoint.trim().length > 0
      );
    }
    return model.value.model_name.trim().length > 0;
  });

  return {
    canProceed,
  };
}

export type UseLlmConfigStepReturn = ReturnType<typeof useLlmConfigStep>;
