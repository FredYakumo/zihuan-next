import { computed, type Ref } from "vue";
import type { ImsBotAdapterSetupConfig } from "../../api/client";

export function useImsBotAdapterConfigStep(model: Ref<ImsBotAdapterSetupConfig>) {
  const canProceed = computed(() => {
    if (model.value.platform === "qq_napcat") {
      return model.value.ws_url.trim().length > 0;
    }
    return false;
  });

  const platformHint = computed(() => {
    switch (model.value.platform) {
      case "wechat":
        return "微信适配器即将支持，敬请期待。";
      case "telegram":
        return "Telegram 适配器即将支持，敬请期待。";
      default:
        return "";
    }
  });

  return {
    canProceed,
    platformHint,
  };
}

export type UseImsBotAdapterConfigStepReturn = ReturnType<typeof useImsBotAdapterConfigStep>;
