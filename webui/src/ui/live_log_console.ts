import { ws } from "../api/ws";
import type { TaskLogEntry } from "../api/types";

const MAX_LOG_ENTRIES = 500;
const STYLE_ID = "zh-live-log-console-style";

let mounted = false;
let frameEl: HTMLDivElement | null = null;
let bodyEl: HTMLDivElement | null = null;
let previewEl: HTMLDivElement | null = null;
let toggleBtnEl: HTMLButtonElement | null = null;
let unsubWs: (() => void) | null = null;
let expanded = false;
const logs: TaskLogEntry[] = [];

function logLevelClass(level: string): string {
  const normalized = level.toUpperCase();
  if (normalized === "ERROR" || normalized === "ERROR_DETAIL") return "zh-live-log-line--error";
  if (normalized === "WARN" || normalized === "WARNING") return "zh-live-log-line--warn";
  if (normalized === "DEBUG") return "zh-live-log-line--debug";
  return "";
}

function formatNow(): string {
  return new Date().toLocaleString("zh-CN", { hour12: false });
}

function ensureStyles(): void {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
.zh-live-log {
  position: fixed;
  left: 20px;
  bottom: 20px;
  width: min(280px, calc(100vw - 24px));
  min-height: 0;
  display: flex;
  flex-direction: column;
  border-radius: 18px;
  overflow: hidden;
  border: 1px solid color-mix(in srgb, var(--border) 76%, transparent 24%);
  background:
    linear-gradient(180deg, color-mix(in srgb, var(--toolbar-bg) 72%, transparent 28%), color-mix(in srgb, var(--bg) 88%, transparent 12%));
  box-shadow: 0 20px 60px color-mix(in srgb, var(--bg) 68%, transparent 32%);
  backdrop-filter: blur(18px);
  z-index: 1300;
}

.zh-live-log--expanded {
  left: 20px;
  right: 20px;
  bottom: 20px;
  width: auto;
  max-width: 1480px;
  height: min(88vh, 980px);
}

.zh-live-log:not(.zh-live-log--expanded) .zh-live-log-bar {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 1;
  padding: 0;
  border: 0;
  background: transparent;
}

.zh-live-log:not(.zh-live-log--expanded) .zh-live-log-title,
.zh-live-log:not(.zh-live-log--expanded) .zh-live-log-badge,
.zh-live-log:not(.zh-live-log--expanded) .zh-live-log-btn-clear,
.zh-live-log:not(.zh-live-log--expanded) .zh-live-log-btn-bottom {
  display: none;
}

.zh-live-log-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent 40%);
  background: color-mix(in srgb, var(--log-stream-bg, #0a0a0a) 86%, var(--toolbar-bg) 14%);
  user-select: none;
  flex-wrap: wrap;
}

.zh-live-log-title {
  flex: 1 1 auto;
  min-width: 0;
  font-family: Consolas, "SFMono-Regular", monospace;
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.zh-live-log-badge {
  flex-shrink: 0;
  color: #4ade80;
  font-family: Consolas, "SFMono-Regular", monospace;
  font-size: 11px;
  animation: zh-live-log-pulse 1.6s ease-in-out infinite;
}

@keyframes zh-live-log-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.zh-live-log-btn {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--border) 70%, transparent 30%);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
}

.zh-live-log-btn:hover {
  background: color-mix(in srgb, var(--border) 30%, transparent 70%);
  color: var(--text);
  border-color: color-mix(in srgb, var(--accent, var(--link)) 24%, var(--border) 76%);
}

.zh-live-log-body {
  flex: 1 1 auto;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 12px 16px;
  background: var(--log-stream-bg, #0a0a0a);
  color: var(--toast-text, var(--text));
  font-family: Consolas, "SFMono-Regular", monospace;
  font-size: 13px;
  line-height: 1.6;
  scroll-behavior: smooth;
}

.zh-live-log-preview {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  min-width: 0;
  min-height: 74px;
  padding: 14px 44px 14px 14px;
  background: var(--log-stream-bg, #0a0a0a);
  color: var(--toast-text, var(--text));
  font-family: Consolas, "SFMono-Regular", monospace;
  font-size: 12px;
  line-height: 1.5;
}

.zh-live-log-preview[hidden] {
  display: none;
}

.zh-live-log-preview-text {
  min-width: 0;
  flex: 1 1 auto;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  white-space: normal;
  overflow-wrap: anywhere;
}

.zh-live-log-preview-level {
  flex: 0 0 auto;
  padding: 2px 7px;
  border-radius: 999px;
  background: color-mix(in srgb, #9ca3af 18%, transparent 82%);
  color: #d1d5db;
  font-size: 10px;
  font-weight: 700;
  line-height: 1.45;
}

.zh-live-log-hint {
  color: var(--text-muted);
  font-size: 13px;
}

.zh-live-log-line {
  display: flex;
  gap: 10px;
  min-width: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.zh-live-log-ts {
  flex-shrink: 0;
  color: #6b7280;
  font-size: 12px;
}

.zh-live-log-level {
  flex-shrink: 0;
  min-width: 64px;
  color: #9ca3af;
}

.zh-live-log-msg {
  flex: 1 1 auto;
  min-width: 0;
}

.zh-live-log-line--error .zh-live-log-level,
.zh-live-log-line--error .zh-live-log-msg {
  color: #f87171;
}

.zh-live-log-line--warn .zh-live-log-level,
.zh-live-log-line--warn .zh-live-log-msg {
  color: #fbbf24;
}

.zh-live-log-line--debug .zh-live-log-level,
.zh-live-log-line--debug .zh-live-log-msg {
  color: #60a5fa;
}

.zh-live-log-level.zh-live-log-line--error {
  color: #f87171;
}

.zh-live-log-level.zh-live-log-line--warn {
  color: #fbbf24;
}

.zh-live-log-level.zh-live-log-line--debug {
  color: #60a5fa;
}

.zh-live-log-preview-level.zh-live-log-line--error {
  background: color-mix(in srgb, #f87171 24%, transparent 76%);
  color: #fca5a5;
}

.zh-live-log-preview-level.zh-live-log-line--warn {
  background: color-mix(in srgb, #fbbf24 24%, transparent 76%);
  color: #fde68a;
}

.zh-live-log-preview-level.zh-live-log-line--debug {
  background: color-mix(in srgb, #60a5fa 24%, transparent 76%);
  color: #93c5fd;
}

@media (max-width: 900px) {
  .zh-live-log {
    left: 12px;
    bottom: 12px;
    width: calc(100vw - 24px);
  }

  .zh-live-log--expanded {
    left: 12px;
    right: 12px;
    bottom: 12px;
    width: auto;
    max-width: none;
    height: min(82vh, 760px);
  }
}
`;
  document.head.appendChild(style);
}

function scrollToBottom(): void {
  if (!bodyEl) return;
  bodyEl.scrollTop = bodyEl.scrollHeight;
}

function render(): void {
  if (!frameEl || !bodyEl || !toggleBtnEl || !previewEl) return;
  frameEl.classList.toggle("zh-live-log--expanded", expanded);
  toggleBtnEl.textContent = expanded ? "缩小" : "⛶";
  toggleBtnEl.title = expanded ? "缩小实时日志" : "展开实时日志";
  toggleBtnEl.setAttribute("aria-label", toggleBtnEl.title);
  previewEl.hidden = expanded;
  bodyEl.hidden = !expanded;

  const latestEntry = logs[logs.length - 1];
  if (!expanded) {
    previewEl.innerHTML = "";
    if (!latestEntry) {
      const hint = document.createElement("div");
      hint.className = "zh-live-log-hint zh-live-log-preview-text";
      hint.textContent = "等待日志输出…";
      previewEl.appendChild(hint);
      return;
    }

    const level = document.createElement("span");
    level.className = `zh-live-log-preview-level ${logLevelClass(latestEntry.level)}`.trim();
    level.textContent = latestEntry.level;

    const msg = document.createElement("span");
    msg.className = "zh-live-log-preview-text";
    msg.textContent = latestEntry.message;

    previewEl.append(level, msg);
    return;
  }

  bodyEl.innerHTML = "";
  if (logs.length === 0) {
    const hint = document.createElement("div");
    hint.className = "zh-live-log-hint";
    hint.textContent = "等待日志输出…";
    bodyEl.appendChild(hint);
    return;
  }

  for (const entry of logs) {
    const row = document.createElement("div");
    row.className = `zh-live-log-line ${logLevelClass(entry.level)}`.trim();

    const ts = document.createElement("span");
    ts.className = "zh-live-log-ts";
    ts.textContent = entry.timestamp;

    const level = document.createElement("span");
    level.className = "zh-live-log-level";
    level.textContent = entry.level;

    const msg = document.createElement("span");
    msg.className = "zh-live-log-msg";
    msg.textContent = entry.message;

    row.append(ts, level, msg);
    bodyEl.appendChild(row);
  }
  scrollToBottom();
}

function pushLog(entry: TaskLogEntry): void {
  logs.push(entry);
  if (logs.length > MAX_LOG_ENTRIES) {
    logs.splice(0, logs.length - MAX_LOG_ENTRIES);
  }
  render();
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

export function mountLiveLogConsole(): void {
  if (mounted) return;
  mounted = true;
  ensureStyles();
  bindSocket();

  frameEl = document.createElement("div");
  frameEl.className = "zh-live-log";

  const bar = document.createElement("div");
  bar.className = "zh-live-log-bar";

  const title = document.createElement("span");
  title.className = "zh-live-log-title";
  title.textContent = "实时日志";

  const badge = document.createElement("span");
  badge.className = "zh-live-log-badge";
  badge.textContent = "● 实时";

  toggleBtnEl = document.createElement("button");
  toggleBtnEl.type = "button";
  toggleBtnEl.className = "zh-live-log-btn";
  toggleBtnEl.addEventListener("click", () => {
    expanded = !expanded;
    render();
  });

  const clearBtn = document.createElement("button");
  clearBtn.type = "button";
  clearBtn.className = "zh-live-log-btn zh-live-log-btn-clear";
  clearBtn.textContent = "清除";
  clearBtn.addEventListener("click", () => {
    logs.splice(0, logs.length);
    render();
  });

  const bottomBtn = document.createElement("button");
  bottomBtn.type = "button";
  bottomBtn.className = "zh-live-log-btn zh-live-log-btn-bottom";
  bottomBtn.textContent = "到底部";
  bottomBtn.addEventListener("click", () => {
    scrollToBottom();
  });

  bar.append(title, badge, toggleBtnEl, clearBtn, bottomBtn);

  bodyEl = document.createElement("div");
  bodyEl.className = "zh-live-log-body";

  previewEl = document.createElement("div");
  previewEl.className = "zh-live-log-preview";

  frameEl.append(bar, previewEl, bodyEl);
  document.body.appendChild(frameEl);
  render();
}
