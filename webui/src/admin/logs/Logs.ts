import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import { clearLogs, enterLogsPage, leaveLogsPage, logLevelClass, logs } from "./logStream";

export function useLogsPage() {
  const bodyEl = ref<HTMLElement | null>(null);

  function scrollToBottom(): void {
    if (bodyEl.value) bodyEl.value.scrollTop = bodyEl.value.scrollHeight;
  }


  function isNearBottom(): boolean {
    const el = bodyEl.value;
    if (!el) return true;
    return el.scrollTop + el.clientHeight >= el.scrollHeight - 30;
  }


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

  return {
    bodyEl,
    logs,
    logLevelClass,
    clearLogs,
  };
}

export type UseLogsPageReturn = ReturnType<typeof useLogsPage>;
