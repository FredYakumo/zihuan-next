<template>
  <section class="page">
    <AdminPageHeader title="运行时实例">
      <t-button variant="outline" @click="load">刷新</t-button>
    </AdminPageHeader>

    <t-card bordered>
      <div v-if="loading" class="empty-state">加载中…</div>
      <div v-else-if="error" class="empty-state">{{ error }}</div>
      <div v-else-if="items.length === 0" class="empty-state">当前没有活动运行时实例。</div>
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

        <t-table
          class="runtime-table"
          row-key="instance_id"
          bordered
          size="small"
          :data="items"
          :columns="columns"
        >
          <template #name="{ row }">
            <strong>{{ row.name }}</strong>
          </template>
          <template #kind="{ row }">
            <t-tag variant="light">{{ row.kind }}</t-tag>
          </template>
          <template #config_id="{ row }">
            <span class="mono" :title="row.config_id">{{ compactId(row.config_id) }}</span>
          </template>
          <template #instance_id="{ row }">
            <span class="mono" :title="row.instance_id">{{ compactId(row.instance_id) }}</span>
          </template>
          <template #started_at="{ row }">{{ formatTime(row.started_at) }}</template>
          <template #duration="{ row }">{{ durationText(row.started_at) }}</template>
          <template #keep_alive="{ row }">{{ row.keep_alive ? "是" : "否" }}</template>
          <template #heartbeat_interval_secs="{ row }">{{ heartbeatText(row.heartbeat_interval_secs) }}</template>
          <template #status="{ row }">
            <t-tag :theme="statusTagTheme(row.status)" variant="light">{{ statusLabel(row.status) }}</t-tag>
          </template>
          <template #actions="{ row }">
            <t-button theme="danger" variant="outline" size="small" @click="forceClose(row.instance_id)">强制关闭</t-button>
          </template>
        </t-table>

        <div class="explorer-pagination">
          <t-button variant="outline" :disabled="page <= 1" @click="go(page - 1)">上一页</t-button>
          <span>{{ page }} / {{ totalPages }} ({{ total }} 条)</span>
          <t-button variant="outline" :disabled="page >= totalPages" @click="go(page + 1)">下一页</t-button>
        </div>
      </template>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useConnectionManager } from "../composables/useConnectionManager";
import { statusTagTheme } from "../model";

const {
  items,
  loading,
  error,
  page,
  pageSize,
  total,
  totalPages,
  runningCount,
  keepAliveCount,
  load,
  go,
  forceClose,
  durationText,
  heartbeatText,
  statusLabel,
  compactId,
  formatTime,
} = useConnectionManager();

const columns = [
  { colKey: "name", title: "名称" },
  { colKey: "kind", title: "实例类型" },
  { colKey: "config_id", title: "Config ID" },
  { colKey: "instance_id", title: "Instance ID" },
  { colKey: "started_at", title: "开始时间" },
  { colKey: "duration", title: "持续时间" },
  { colKey: "keep_alive", title: "长连接" },
  { colKey: "heartbeat_interval_secs", title: "心跳" },
  { colKey: "status", title: "状态" },
  { colKey: "actions", title: "操作" },
];
</script>

<style scoped lang="scss">
@use "../styles/connection-manager" as *;
</style>
