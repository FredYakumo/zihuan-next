import { computed, ref } from "vue";

import type { DetailedInstallCommandResult } from "../../api/client";

export function useDetailedInstallCommandResult(result: () => DetailedInstallCommandResult) {
  const copied = ref<"command" | "connections" | null>(null);
  const copyError = ref<string | null>(null);
  const connectionsJson = computed(() => JSON.stringify(result().connections, null, 2));

  async function copyText(value: string, target: "command" | "connections") {
    copyError.value = null;
    try {
      await navigator.clipboard.writeText(value);
      copied.value = target;
      window.setTimeout(() => {
        if (copied.value === target) copied.value = null;
      }, 1600);
    } catch (error) {
      copyError.value = `复制失败：${error instanceof Error ? error.message : String(error)}`;
    }
  }

  return { copied, copyError, connectionsJson, copyText };
}
