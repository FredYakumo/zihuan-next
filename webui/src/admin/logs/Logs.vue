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
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import AdminPageHeader from "../components/AdminPageHeader.vue";
import { clearLogs, enterLogsPage, leaveLogsPage, logLevelClass, logs } from "./logStream";

const bodyEl = ref<HTMLElement | null>(null);

function scrollToBottom(): void {
  if (bodyEl.value) bodyEl.value.scrollTop = bodyEl.value.scrollHeight;
}

// 距底部 30px 内视为贴底：给平滑滚动动画和像素取整留容差
function isNearBottom(): boolean {
  const el = bodyEl.value;
  if (!el) return true;
  return el.scrollTop + el.clientHeight >= el.scrollHeight - 30;
}

// 追踪最后一条的 seq 而非数组长度：日志达到 500 条上限后 push+裁剪使长度不变，
// 监听 length 会在稳定状态下漏掉新日志
watch(
  () => logs.value[logs.value.length - 1]?.seq,
  async () => {
    // watch 默认 flush: 'pre'，回调时 DOM 尚未更新，读到的是新日志渲染前用户所处的位置
    const followBottom = isNearBottom();
    await nextTick();
    if (followBottom) scrollToBottom();
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
@use "../tasks/tasks" as *;

.log-page-body {
  height: min(82vh, 920px);
}
</style>
