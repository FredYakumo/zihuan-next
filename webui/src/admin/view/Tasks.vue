<template>
  <section class="page tasks-page">
    <div class="page-hero">
      <h2>任务管理器</h2>
    </div>

    <section class="panel">
      <div class="split-header">
        <div>
          <h3>任务列表</h3>
          <p class="muted">共 {{ taskItems.length }} 个。</p>
        </div>
        <div class="inline-actions">
          <button class="btn ghost" @click="load">刷新</button>
          <button class="btn" @click="clearFinished">清理已结束任务</button>
        </div>
      </div>
      <div class="list" style="margin-top: 16px;">
        <div v-if="taskItems.length === 0" class="empty-state">还没有任务。</div>
        <article
          v-for="task in taskItems"
          :key="task.id"
          class="record"
        >
          <div class="split-header">
            <div>
              <h4>{{ task.graph_name }}</h4>
              <div class="record-meta">
                <span>{{ formatTime(task.start_time) }}</span>
                <span v-if="task.file_path">{{ task.file_path }}</span>
              </div>
            </div>
            <div style="display:flex;align-items:center;gap:8px;">
              <span class="badge task-type-badge" :class="task.task_type === 'agent_service' ? 'task-type-agent' : 'task-type-graph'">
                {{ task.task_type === "agent_service" ? "Agent 服务" : "节点图" }}
              </span>
              <span class="badge" :class="statusTone(task.status)">{{ task.status }}</span>
            </div>
          </div>
          <div class="panel-actions" style="margin-top: 14px;">
            <button class="btn" :disabled="!task.is_running" @click="stopTask(task.id)">停止</button>
            <button class="btn" :disabled="!task.can_rerun" @click="rerunTask(task.id)">重跑</button>
            <button class="btn ghost" @click="openLogViewer(task)">查看日志</button>
          </div>
        </article>
      </div>
    </section>

    <section class="task-terminal">
      <div class="task-terminal-bar">
        <span class="task-terminal-title">实时日志</span>
        <span class="task-terminal-live">● 实时</span>
        <button class="task-terminal-btn" title="清除" @click="clearTerminal">清除</button>
        <button class="task-terminal-btn" title="滚到底部" @click="scrollToBottom">↓</button>
      </div>
      <div ref="terminalEl" class="task-terminal-body">
        <div v-if="logs.length === 0" class="task-terminal-hint">等待日志输出…</div>
        <div
          v-for="(entry, index) in logs"
          :key="`${entry.timestamp}-${index}`"
          class="task-terminal-line"
          :class="logLevelClass(entry.level)"
        >
          <span class="task-terminal-ts">{{ entry.timestamp }}</span>
          <span class="task-terminal-level">{{ entry.level }}</span>
          <span class="task-terminal-msg">{{ entry.message }}</span>
        </div>
      </div>
    </section>

    <!-- Log viewer dialog -->
    <div v-if="logViewerTask" class="connection-picker-backdrop" @click.self="closeLogViewer">
      <div class="connection-picker-dialog log-viewer-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>日志 — {{ logViewerTask.graph_name }}</h3>
          <button class="task-terminal-btn" @click="closeLogViewer">✕</button>
        </div>

        <!-- Controls -->
        <div class="log-viewer-controls">
          <label class="log-viewer-label">
            日期
            <input
              v-model="logFilter.date"
              type="date"
              class="log-viewer-input"
              @change="fetchLogs(true)"
            />
          </label>
          <label class="log-viewer-label">
            每页条数
            <select v-model="logFilter.limit" class="log-viewer-input" @change="fetchLogs(true)">
              <option :value="50">50</option>
              <option :value="100">100</option>
              <option :value="200">200</option>
              <option :value="500">500</option>
            </select>
          </label>
          <div class="log-viewer-pagination">
            <span class="muted" style="font-size:13px;">第 {{ currentPage + 1 }} / {{ totalPages }} 页（共 {{ logTotal }} 条）</span>
            <button class="task-terminal-btn" :disabled="currentPage === 0" @click="prevPage">‹ 上一页</button>
            <button class="task-terminal-btn" :disabled="currentPage + 1 >= totalPages" @click="nextPage">下一页 ›</button>
          </div>
          <button class="task-terminal-btn" style="margin-left:auto;" @click="fetchLogs(true)">刷新</button>
        </div>

        <!-- Log body -->
        <div ref="logViewerBody" class="task-terminal-body log-viewer-body">
          <div v-if="logViewerLoading" class="task-terminal-hint">加载中…</div>
          <div v-else-if="logViewerEntries.length === 0" class="task-terminal-hint">暂无日志。</div>
          <div
            v-for="(entry, index) in logViewerEntries"
            :key="`${entry.timestamp}-${index}`"
            class="task-terminal-line"
            :class="logLevelClass(entry.level)"
          >
            <span class="task-terminal-ts">{{ entry.timestamp }}</span>
            <span class="task-terminal-level">{{ entry.level }}</span>
            <span class="task-terminal-msg">{{ entry.message }}</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";

import { tasks, type TaskEntry, type TaskLogEntry } from "../../api/client";
import { ws } from "../../api/ws";
import { formatTime, statusTone } from "../model";

const taskItems = ref<TaskEntry[]>([]);
const logs = ref<TaskLogEntry[]>([]);
const terminalEl = ref<HTMLElement | null>(null);

function logLevelClass(level: string): string {
  const l = level.toUpperCase();
  if (l === "ERROR" || l === "ERROR_DETAIL") return "task-terminal-line--error";
  if (l === "WARN" || l === "WARNING") return "task-terminal-line--warn";
  if (l === "DEBUG") return "task-terminal-line--debug";
  return "";
}

async function scrollToBottom() {
  await nextTick();
  if (terminalEl.value) {
    terminalEl.value.scrollTop = terminalEl.value.scrollHeight;
  }
}

function clearTerminal() {
  logs.value = [];
}

async function load() {
  taskItems.value = await tasks.list();
}

async function stopTask(id: string) {
  await tasks.stop(id);
  await load();
}

async function rerunTask(id: string) {
  await tasks.rerun(id);
  await load();
}

async function clearFinished() {
  await tasks.clearFinished();
  await load();
}

// ── Log viewer ─────────────────────────────────────────────────────────────

const logViewerTask = ref<TaskEntry | null>(null);
const logViewerEntries = ref<TaskLogEntry[]>([]);
const logViewerLoading = ref(false);
const logTotal = ref(0);
const logViewerBody = ref<HTMLElement | null>(null);

const logFilter = ref<{ date: string; limit: number; page: number }>({
  date: "",
  limit: 100,
  page: 0,
});

const totalPages = computed(() =>
  logTotal.value === 0 ? 1 : Math.ceil(logTotal.value / logFilter.value.limit)
);
const currentPage = computed(() => logFilter.value.page);

async function fetchLogs(resetPage = false) {
  if (!logViewerTask.value) return;
  if (resetPage) logFilter.value.page = 0;
  logViewerLoading.value = true;
  try {
    const offset = logFilter.value.page * logFilter.value.limit;
    const result = await tasks.logs(logViewerTask.value.id, {
      date: logFilter.value.date || undefined,
      limit: logFilter.value.limit,
      offset,
    });
    logViewerEntries.value = result.entries;
    logTotal.value = result.total;
    await nextTick();
    if (logViewerBody.value) logViewerBody.value.scrollTop = 0;
  } finally {
    logViewerLoading.value = false;
  }
}

function openLogViewer(task: TaskEntry) {
  logViewerTask.value = task;
  logFilter.value = { date: "", limit: 100, page: 0 };
  logViewerEntries.value = [];
  logTotal.value = 0;
  void fetchLogs();
}

function closeLogViewer() {
  logViewerTask.value = null;
}

function prevPage() {
  if (logFilter.value.page > 0) {
    logFilter.value.page--;
    void fetchLogs();
  }
}

function nextPage() {
  if (logFilter.value.page + 1 < totalPages.value) {
    logFilter.value.page++;
    void fetchLogs();
  }
}

// ── WebSocket ──────────────────────────────────────────────────────────────

let unsubWs: (() => void) | null = null;

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`任务页面加载失败: ${(error as Error).message}`);
  });

  unsubWs = ws.onMessage((msg) => {
    if (msg.type === "TaskStarted") {
      load().catch(console.error);
      const entry = { timestamp: new Date().toLocaleString("zh-CN", { hour12: false }), level: "INFO", message: `任务 "${msg.graph_name}" 已启动 (ID: ${msg.task_id})` };
      logs.value.push(entry);
      void scrollToBottom();
      return;
    }
    if (msg.type === "TaskFinished") {
      load().catch(console.error);
      const entry = {
        timestamp: new Date().toLocaleString("zh-CN", { hour12: false }),
        level: msg.success ? "INFO" : "ERROR",
        message: msg.success ? `任务已完成 (ID: ${msg.task_id})` : `任务失败: ${msg.error ?? "未知错误"} (ID: ${msg.task_id})`,
      };
      logs.value.push(entry);
      void scrollToBottom();
      return;
    }
    if (msg.type === "TaskStopped") {
      load().catch(console.error);
      const entry = { timestamp: new Date().toLocaleString("zh-CN", { hour12: false }), level: "WARN", message: `任务已停止 (ID: ${msg.task_id})` };
      logs.value.push(entry);
      void scrollToBottom();
      return;
    }
    if (msg.type === "LogMessage") {
      logs.value.push({ timestamp: msg.timestamp, level: msg.level, message: msg.message });
      void scrollToBottom();
    }
  });
});

onUnmounted(() => {
  unsubWs?.();
});
</script>
