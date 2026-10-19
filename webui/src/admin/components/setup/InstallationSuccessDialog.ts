import { computed, ref } from "vue";

export interface InstallationSuccessDialogProps {
  visible: boolean;
  title: string;
  description: string;
  installCommand: string | null;
  connectionConfig: unknown;
}

export function useInstallationSuccessDialog(props: InstallationSuccessDialogProps) {
  const copied = ref<"command" | "connections" | null>(null);
  const copyError = ref<string | null>(null);
  const connectionConfigJson = computed(() => JSON.stringify(props.connectionConfig, null, 2));

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

  return { copied, copyError, connectionConfigJson, copyText };
}
