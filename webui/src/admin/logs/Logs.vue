<template>
  <section class="page">
    <AdminPageHeader title="日志">
      <t-button variant="outline" @click="clearLogs">清除</t-button>
    </AdminPageHeader>

    <t-card bordered>
      <div ref="bodyEl" class="task-terminal-body log-page-body">
        <div v-if="logs.length === 0" class="task-terminal-hint">等待日志输出…</div>
        <div
          v-for="entry in logs"
          :key="entry.seq"
          class="task-terminal-line"
          :class="logLevelClass(entry.level)"
        >
          <span class="task-terminal-ts">{{ entry.timestamp }}</span>
          <span class="task-terminal-level">{{ entry.level }}</span>
          <span class="task-terminal-msg">{{ entry.message }}</span>
        </div>
      </div>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useLogsPage } from "./Logs";

const { bodyEl, logs, logLevelClass, clearLogs } = useLogsPage();
</script>

<style scoped lang="scss">
@use "./logs" as *;
</style>
