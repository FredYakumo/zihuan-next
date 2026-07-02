<template>
  <section class="page">
    <div class="page-hero">
      <h2>运行时实例</h2>
      <div class="hero-actions">
        <button class="btn ghost" @click="load">刷新</button>
      </div>
    </div>

    <section class="panel">
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

        <div class="runtime-table-wrap">
          <table class="explorer-table runtime-table">
            <colgroup>
              <col class="col-name" />
              <col class="col-kind" />
              <col class="col-config" />
              <col class="col-instance" />
              <col class="col-started" />
              <col class="col-duration" />
              <col class="col-keepalive" />
              <col class="col-heartbeat" />
              <col class="col-status" />
              <col class="col-actions" />
            </colgroup>
            <thead>
              <tr>
                <th>名称</th>
                <th>实例类型</th>
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
                <td class="runtime-cell-name">
                  <div class="runtime-name-cell">
                    <strong>{{ item.name }}</strong>
                  </div>
                </td>
                <td class="runtime-cell-nowrap">
                  <span class="badge">{{ item.kind }}</span>
                </td>
                <td class="mono runtime-cell-ellipsis runtime-cell-nowrap" :title="item.config_id">{{ compactId(item.config_id) }}</td>
                <td class="mono runtime-cell-ellipsis runtime-cell-nowrap" :title="item.instance_id">{{ compactId(item.instance_id) }}</td>
                <td class="runtime-cell-nowrap">{{ formatTime(item.started_at) }}</td>
                <td class="runtime-cell-nowrap">{{ durationText(item.started_at) }}</td>
                <td class="runtime-cell-center runtime-cell-nowrap">{{ item.keep_alive ? "是" : "否" }}</td>
                <td class="runtime-cell-nowrap">{{ heartbeatText(item.heartbeat_interval_secs) }}</td>
                <td class="runtime-cell-nowrap">
                  <span class="badge" :class="statusTone(item.status)">{{ statusLabel(item.status) }}</span>
                </td>
                <td class="runtime-cell-actions">
                  <button class="btn warn runtime-action-btn" @click="forceClose(item.instance_id)">强制关闭</button>
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
import { useConnectionManager } from "../composables/useConnectionManager";

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
  statusTone,
} = useConnectionManager();
</script>

<style scoped lang="scss">
@use "../styles/connection-manager" as *;
</style>
