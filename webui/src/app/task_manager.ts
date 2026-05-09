import { tasks as tasksApi } from "../api/client";
import type { TaskEntry, TaskLogEntry } from "../api/types";

type TaskListener = (tasks: TaskEntry[]) => void;

export class TaskManagerStore {
  private taskList: TaskEntry[] = [];
  private listeners: TaskListener[] = [];
  private refreshTimer: ReturnType<typeof setInterval> | null = null;

  start(): void {
    if (!this.refreshTimer) {
      this.refreshTimer = setInterval(() => {
        this.refresh().catch((error) => {
          console.error("Failed to poll task list", error);
        });
      }, 5000);
    }

    this.refresh().catch((error) => {
      console.error("Failed to load initial task list", error);
    });
  }

  stop(): void {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = null;
    }
  }

  async refresh(): Promise<TaskEntry[]> {
    const list = await tasksApi.list();
    this.taskList = list;
    this.emit();
    return this.getTasks();
  }

  subscribe(listener: TaskListener): () => void {
    this.listeners.push(listener);
    listener(this.getTasks());
    return () => {
      this.listeners = this.listeners.filter((entry) => entry !== listener);
    };
  }

  getTasks(): TaskEntry[] {
    return [...this.taskList];
  }

  getRunningTasks(): TaskEntry[] {
    return this.taskList.filter((task) => task.is_running);
  }

  getRunningTaskForSession(sessionId: string | null): TaskEntry | null {
    if (!sessionId) return null;
    return this.taskList.find((task) => task.is_running && task.graph_session_id === sessionId) ?? null;
  }

  async stopTask(taskId: string): Promise<void> {
    await tasksApi.stop(taskId);
    await this.refresh();
  }

  async rerunTask(taskId: string): Promise<{ task_id: string }> {
    const result = await tasksApi.rerun(taskId);
    await this.refresh();
    return result;
  }

  async getTaskLogs(taskId: string): Promise<TaskLogEntry[]> {
    const result = await tasksApi.logs(taskId);
    return result.entries;
  }

  async clearFinished(): Promise<{ ok: boolean; cleared: number }> {
    const result = await tasksApi.clearFinished();
    await this.refresh();
    return result;
  }

  private emit(): void {
    const snapshot = this.getTasks();
    for (const listener of this.listeners) {
      listener(snapshot);
    }
  }
}

export function taskDisplayName(task: Pick<TaskEntry, "graph_name" | "is_workflow_set">): string {
  return task.is_workflow_set ? `${task.graph_name} [工作流集]` : task.graph_name;
}

export function formatTaskTimestamp(value: string | null): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function formatTaskDuration(task: Pick<TaskEntry, "start_time" | "end_time" | "is_running" | "duration_ms">): string {
  if (!task.is_running && typeof task.duration_ms === "number") {
    return formatDurationMs(task.duration_ms);
  }
  const start = new Date(task.start_time).getTime();
  if (Number.isNaN(start)) return "-";
  const end = task.is_running || !task.end_time ? Date.now() : new Date(task.end_time).getTime();
  if (Number.isNaN(end)) return "-";
  return formatDurationMs(end - start);
}

function formatDurationMs(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}小时 ${minutes}分 ${seconds}秒`;
  }
  if (minutes > 0) {
    return `${minutes}分 ${seconds}秒`;
  }
  return `${seconds}秒`;
}
