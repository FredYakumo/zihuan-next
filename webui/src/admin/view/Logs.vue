<template>
  <section class="page">
    <div class="page-hero">
      <h2>日志</h2>
      <button class="btn ghost" @click="clearLogs">清除</button>
    </div>

    <section class="panel">
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
    </section>
  </section>
</template>

<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";

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
