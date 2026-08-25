import { toRefs } from "vue";

import type { SetupProgressEvent } from "../../api/client";

export interface UseInstallationProgressProps {
  taskId: string;
  logs: SetupProgressEvent[];
  error: string | null;
}

export function useInstallationProgress(props: UseInstallationProgressProps) {
  const { logs, error } = toRefs(props);
  return { logs, error };
}

export type UseInstallationProgressReturn = ReturnType<typeof useInstallationProgress>;
