import { ref } from "vue";

import { ws } from "../../api/ws";
import type { TaskLogEntry } from "../../api/types";

const MAX_LOG_ENTRIES = 500;
const BADGE_SETTING_KEY = "zh-log-error-badge-enabled";

export const logs = ref<TaskLogEntry[]>([]);
export const errorCount = ref(0);
export const logErrorBadgeEnabled = ref<boolean>(localStorage.getItem(BADGE_SETTING_KEY) !== "false");

let initialized = false;
let viewingLogs = false;
let unsubWs: (() => void) | null = null;

function isErrorLevel(level: string): boolean {
  const normalized = level.toUpperCase();
  return normalized === "ERROR" || normalized === "ERROR_DETAIL";
}

export function logLevelClass(level: string): string {
  const normalized = level.toUpperCase();
  if (normalized === "ERROR" || normalized === "ERROR_DETAIL") return "task-terminal-line--error";
  if (normalized === "WARN" || normalized === "WARNING") return "task-terminal-line--warn";
  if (normalized === "DEBUG") return "task-terminal-line--debug";
  return "";
}

function formatNow(): string {
  return new Date().toLocaleString("zh-CN", { hour12: false });
}

function pushLog(entry: TaskLogEntry): void {
  logs.value.push(entry);
  if (logs.value.length > MAX_LOG_ENTRIES) {
    logs.value.splice(0, logs.value.length - MAX_LOG_ENTRIES);
  }
  if (isErrorLevel(entry.level) && !viewingLogs) {
    errorCount.value += 1;
  }
}

function bindSocket(): void {
  if (unsubWs) return;
  unsubWs = ws.onMessage((msg) => {
    if (msg.type === "TaskStarted") {
      pushLog({
        timestamp: formatNow(),
        level: "INFO",
        message: `任务 "${msg.graph_name}" 已启动 (ID: ${msg.task_id})`,
      });
      return;
    }
    if (msg.type === "TaskFinished") {
      pushLog({
        timestamp: formatNow(),
        level: msg.success ? "INFO" : "ERROR",
        message: msg.success ? `任务已完成 (ID: ${msg.task_id})` : `任务失败: ${msg.error ?? "未知错误"} (ID: ${msg.task_id})`,
      });
      return;
    }
    if (msg.type === "TaskStopped") {
      pushLog({
        timestamp: formatNow(),
        level: "WARN",
        message: `任务已停止 (ID: ${msg.task_id})`,
      });
      return;
    }
    if (msg.type === "LogMessage") {
      pushLog({
        timestamp: msg.timestamp,
        level: msg.level,
        message: msg.message,
      });
    }
  });
}

export function initLogStream(): void {
  if (initialized) return;
  initialized = true;
  bindSocket();
}

export function clearLogs(): void {
  logs.value.splice(0, logs.value.length);
}

export function enterLogsPage(): void {
  viewingLogs = true;
  errorCount.value = 0;
}

export function leaveLogsPage(): void {
  viewingLogs = false;
}

export function setLogErrorBadgeEnabled(enabled: boolean): void {
  logErrorBadgeEnabled.value = enabled;
  localStorage.setItem(BADGE_SETTING_KEY, String(enabled));
}
