import { nextTick, ref, watch } from "vue";

import type { SetupProgressEvent } from "../../../api/client";
import { useInstallationProgress } from "../../setup/useInstallationProgress";

export interface InstallationProgressProps {
  taskId: string;
  logs: SetupProgressEvent[];
  error: string | null;
  title: string;
  subtitle: string;
  backLabel: string;
}

export function useInstallationProgressView(props: InstallationProgressProps) {
  const { logs, error } = useInstallationProgress(props);
  const consoleElement = ref<HTMLElement | null>(null);

  watch(
    () => logs.value.length,
    async () => {
      await nextTick();
      if (consoleElement.value) {
        consoleElement.value.scrollTop = consoleElement.value.scrollHeight;
      }
    },
  );

  return { logs, error, consoleElement };
}
