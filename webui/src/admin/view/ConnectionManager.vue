<template>
  <section class="page">
    <div class="page-hero">
      <h2>连接管理器</h2>
      <div class="hero-actions">
        <button class="btn ghost" @click="load">刷新</button>
      </div>
    </div>

    <section class="panel">
      <div v-if="loading" class="empty-state">加载中…</div>
      <div v-else-if="error" class="empty-state">{{ error }}</div>
      <div v-else-if="items.length === 0" class="empty-state">当前没有活动连接实例。</div>
      <template v-else>
        <div class="runtime-summary">
          <div class="runtime-stat">
            <span class="muted">当前实例</span>
            <strong>{{ total }}</strong>
          </div>
          <div class="runtime-stat">
            <span class="muted">运行中</span>
            <strong>{{ runningCount }}</strong>
          </div>
          <div class="runtime-stat">
            <span class="muted">长连接</span>
            <strong>{{ keepAliveCount }}</strong>
          </div>
        </div>

        <div class="runtime-table-wrap">
          <table class="task-table runtime-table">
            <thead>
              <tr>
                <th>连接</th>
                <th>类型</th>
                <th>Config ID</th>
                <th>Instance ID</th>
                <th>开始时间</th>
                <th>持续时间</th>
                <th>长连接</th>
                <th>心跳</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in items" :key="item.instance_id">
                <td>
                  <div class="runtime-name-cell">
                    <strong>{{ item.name }}</strong>
                  </div>
                </td>
                <td>
                  <span class="badge">{{ item.kind }}</span>
                </td>
                <td class="mono" :title="item.config_id">{{ compactId(item.config_id) }}</td>
                <td class="mono" :title="item.instance_id">{{ compactId(item.instance_id) }}</td>
                <td>{{ formatTime(item.started_at) }}</td>
                <td>{{ durationText(item.started_at) }}</td>
                <td>{{ item.keep_alive ? "是" : "否" }}</td>
                <td>{{ heartbeatText(item.heartbeat_interval_secs) }}</td>
                <td>
                  <span class="badge" :class="statusTone(item.status)">{{ statusLabel(item.status) }}</span>
                </td>
                <td>
                  <button class="btn warn connection-card-compact-btn" @click="forceClose(item.instance_id)">强制关闭</button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="explorer-pagination">
          <button class="btn ghost" :disabled="page <= 1" @click="go(page - 1)">上一页</button>
          <span>{{ page }} / {{ totalPages }} ({{ total }} 条)</span>
          <button class="btn ghost" :disabled="page >= totalPages" @click="go(page + 1)">下一页</button>
        </div>
      </template>
    </section>
  </section>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import { system, type RuntimeConnectionInstanceSummary } from "../../api/client";
import { compactId, formatTime, statusTone } from "../model";

const items = ref<RuntimeConnectionInstanceSummary[]>([]);
const loading = ref(false);
const error = ref("");
const page = ref(1);
const pageSize = ref(20);
const total = ref(0);

const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize.value)));
const runningCount = computed(() => items.value.filter((item) => item.status === "running").length);
const keepAliveCount = computed(() => items.value.filter((item) => item.keep_alive).length);

let timer: number | null = null;

async function load() {
  loading.value = true;
  error.value = "";
  try {
    const response = await system.connections.listRuntimeInstances({
      page: page.value,
      page_size: pageSize.value,
    });
    items.value = response.items;
    total.value = response.total;
  } catch (err) {
    console.error(err);
    error.value = `加载失败: ${(err as Error).message}`;
  } finally {
    loading.value = false;
  }
}

async function go(nextPage: number) {
  page.value = Math.min(Math.max(1, nextPage), totalPages.value);
  await load();
}

async function forceClose(instanceId: string) {
  if (!window.confirm("确认强制关闭这个连接实例吗？")) {
    return;
  }
  await system.connections.closeRuntimeInstance(instanceId);
  await load();
}

function durationText(startedAt: string): string {
  const start = new Date(startedAt).getTime();
  if (Number.isNaN(start)) {
    return "未知";
  }
  const seconds = Math.max(0, Math.floor((Date.now() - start) / 1000));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainSeconds = seconds % 60;
  if (hours > 0) return `${hours}h ${minutes}m ${remainSeconds}s`;
  if (minutes > 0) return `${minutes}m ${remainSeconds}s`;
  return `${remainSeconds}s`;
}

function heartbeatText(intervalSecs: number | null): string {
  if (intervalSecs == null || intervalSecs <= 0) {
    return "未启用";
  }
  return `${intervalSecs}s`;
}

function statusLabel(status: RuntimeConnectionInstanceSummary["status"]): string {
  switch (status) {
    case "running":
      return "运行中";
    case "idle":
      return "空闲";
    case "closing":
      return "关闭中";
    case "error":
      return "错误";
    default:
      return status;
  }
}

onMounted(() => {
  load();
  timer = window.setInterval(() => {
    items.value = [...items.value];
  }, 1000);
});

onBeforeUnmount(() => {
  if (timer != null) {
    window.clearInterval(timer);
  }
});
</script>

<style scoped>
.runtime-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  margin-bottom: 18px;
}

.runtime-stat {
  min-width: 120px;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: 16px;
  background: color-mix(in srgb, var(--admin-bg-soft) 88%, transparent 12%);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.runtime-stat strong {
  font-size: 22px;
  line-height: 1;
}

.runtime-table-wrap {
  overflow-x: auto;
}

.runtime-table {
  min-width: 1120px;
}

.runtime-table th {
  white-space: nowrap;
}

.runtime-table td {
  vertical-align: middle;
}

.runtime-name-cell {
  min-width: 150px;
}

.runtime-name-cell strong {
  display: block;
  line-height: 1.35;
}
</style>
