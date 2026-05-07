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
        <table class="task-table">
          <thead>
            <tr>
              <th>连接名</th>
              <th>类型</th>
              <th>Config ID</th>
              <th>Instance ID</th>
              <th>开始时间</th>
              <th>持续时间</th>
              <th>维持长连接</th>
              <th>心跳</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in items" :key="item.instance_id">
              <td>{{ item.name }}</td>
              <td>{{ item.kind }}</td>
              <td class="mono">{{ item.config_id }}</td>
              <td class="mono">{{ item.instance_id }}</td>
              <td>{{ formatTime(item.started_at) }}</td>
              <td>{{ durationText(item.started_at) }}</td>
              <td>{{ item.keep_alive ? "是" : "否" }}</td>
              <td>{{ heartbeatText(item.heartbeat_interval_secs) }}</td>
              <td>
                <span class="badge" :class="statusTone(item.status)">{{ item.status }}</span>
              </td>
              <td>
                <button class="btn warn" @click="forceClose(item.instance_id)">强制关闭</button>
              </td>
            </tr>
          </tbody>
        </table>

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
import { formatTime, statusTone } from "../model";

const items = ref<RuntimeConnectionInstanceSummary[]>([]);
const loading = ref(false);
const error = ref("");
const page = ref(1);
const pageSize = ref(20);
const total = ref(0);

const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize.value)));

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
