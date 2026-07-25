<template>
  <section class="page">
    <AdminPageHeader title="日志">
      <t-button variant="outline" @click="clearLogs">清除</t-button>
    </AdminPageHeader>

    <t-card bordered>
      <div ref="bodyEl" class="task-terminal-body log-page-body">
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
    </t-card>
  </section>
</template>

<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import AdminPageHeader from "../components/AdminPageHeader.vue";
import { clearLogs, enterLogsPage, leaveLogsPage, logLevelClass, logs } from "../state/logStream";

const bodyEl = ref<HTMLElement | null>(null);

function scrollToBottom(): void {
  if (bodyEl.value) bodyEl.value.scrollTop = bodyEl.value.scrollHeight;
}

watch(
  () => logs.value.length,
  async () => {
    await nextTick();
    scrollToBottom();
  }
);

onMounted(async () => {
  enterLogsPage();
  await nextTick();
  scrollToBottom();
});

onUnmounted(() => {
  leaveLogsPage();
});
</script>

<style scoped lang="scss">
@use "../styles/tasks" as *;

.log-page-body {
  height: min(82vh, 920px);
}
</style>
