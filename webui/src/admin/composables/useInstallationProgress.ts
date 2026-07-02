import type { SetupProgressEvent } from "../../api/client";

export interface UseInstallationProgressProps {
  taskId: string;
  logs: SetupProgressEvent[];
  error: string | null;
}

export function useInstallationProgress(props: UseInstallationProgressProps) {
  return {
    logs: props.logs,
    error: props.error,
  };
}

export type UseInstallationProgressReturn = ReturnType<typeof useInstallationProgress>;
