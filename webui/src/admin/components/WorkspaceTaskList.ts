import type { WorkspaceTask } from "../../api/client";

export interface WorkspaceTaskListProps {
  tasks: WorkspaceTask[];
  interrupted?: boolean;
  compact?: boolean;
}

export function useWorkspaceTaskList(_props: WorkspaceTaskListProps) {
  function taskColor(status: WorkspaceTask["status"], interrupted = false): string {
    if ((interrupted || status === "interrupted") && status !== "completed") return "#9aa0a6";
    if (status === "completed") return "#2ba471";
    if (status === "pending") return "#eab308";
    return "var(--admin-text-muted)";
  }

  return { taskColor };
}
