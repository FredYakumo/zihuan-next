import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import { system, type RuntimeConnectionInstanceSummary } from "../../api/client";
import { compactId, formatTime, statusTone } from "../model";


export function useConnectionManager() {
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
    if (!window.confirm("确认强制关闭这个运行时实例吗？")) {
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

  return {
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
  };
}

export type UseConnectionManagerReturn = ReturnType<typeof useConnectionManager>;
