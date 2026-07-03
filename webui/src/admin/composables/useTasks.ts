import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import { tasks, type TaskEntry, type TaskLogEntry } from "../../api/client";
import { formatTaskDuration } from "../../app/task_manager";
import { ws } from "../../api/ws";
import { formatTime, statusTone } from "../model";


export function useTasks() {
  const taskItems = ref<TaskEntry[]>([]);
  const pageSize = ref(10);
  const listPage = ref(1);
  const listPageInput = ref(1);
  const selectedTaskIds = ref(new Set<string>());

  const listTotalPages = computed(() =>
    Math.max(1, Math.ceil(taskItems.value.length / pageSize.value)),
  );
  const pagedTaskItems = computed(() => {
    const start = (listPage.value - 1) * pageSize.value;
    return taskItems.value.slice(start, start + pageSize.value);
  });
  const allPagedTasksSelected = computed(
    () =>
      pagedTaskItems.value.length > 0 &&
      pagedTaskItems.value.every((task) => selectedTaskIds.value.has(task.id)),
  );
  const somePagedTasksSelected = computed(() =>
    pagedTaskItems.value.some((task) => selectedTaskIds.value.has(task.id)),
  );

  watch(pageSize, () => {
    goToListPage(Math.min(listPage.value, listTotalPages.value));
  });

  watch(
    () => taskItems.value.length,
    () => {
      goToListPage(Math.min(listPage.value, listTotalPages.value));
    },
  );

  function goToListPage(page: number) {
    const normalized = Math.min(Math.max(1, page || 1), listTotalPages.value);
    listPage.value = normalized;
    listPageInput.value = normalized;
  }

  function jumpToListPage() {
    goToListPage(listPageInput.value);
  }

  async function load() {
    taskItems.value = await tasks.list();
    const existingIds = new Set(taskItems.value.map((task) => task.id));
    selectedTaskIds.value = new Set(
      [...selectedTaskIds.value].filter((id) => existingIds.has(id)),
    );
  }

  async function stopTask(id: string) {
    await tasks.stop(id);
    await load();
  }

  async function rerunTask(id: string) {
    await tasks.rerun(id);
    await load();
  }

  async function clearFinished() {
    await tasks.clearFinished();
    await load();
  }

  function toggleTaskSelection(id: string) {
    const next = new Set(selectedTaskIds.value);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selectedTaskIds.value = next;
  }

  function toggleCurrentPageSelection() {
    const next = new Set(selectedTaskIds.value);
    if (allPagedTasksSelected.value) {
      for (const task of pagedTaskItems.value) next.delete(task.id);
    } else {
      for (const task of pagedTaskItems.value) next.add(task.id);
    }
    selectedTaskIds.value = next;
  }

  async function deleteSingleTask(task: TaskEntry) {
    if (!confirm(`永久删除任务“${task.graph_name}”？对应日志也会被清除。`)) return;
    await tasks.delete(task.id);
    await load();
  }

  async function deleteSelectedTasks() {
    const ids = [...selectedTaskIds.value];
    if (ids.length === 0) return;
    if (!confirm(`永久删除选中的 ${ids.length} 个任务？对应日志也会被清除。`)) return;
    await tasks.deleteBatch(ids);
    selectedTaskIds.value = new Set();
    await load();
  }

  function logLevelClass(level: string): string {
    const normalized = level.toUpperCase();
    if (normalized === "ERROR" || normalized === "ERROR_DETAIL") {
      return "task-terminal-line--error";
    }
    if (normalized === "WARN" || normalized === "WARNING") {
      return "task-terminal-line--warn";
    }
    if (normalized === "DEBUG") {
      return "task-terminal-line--debug";
    }
    return "";
  }

  const logViewerTask = ref<TaskEntry | null>(null);
  const logViewerEntries = ref<TaskLogEntry[]>([]);
  const logViewerLoading = ref(false);
  const logTotal = ref(0);
  const logViewerBody = ref<HTMLElement | null>(null);

  const logFilter = ref<{ date: string; limit: number; page: number }>({
    date: "",
    limit: 100,
    page: 0,
  });

  const logTotalPages = computed(() =>
    logTotal.value === 0 ? 1 : Math.ceil(logTotal.value / logFilter.value.limit),
  );
  const currentPage = computed(() => logFilter.value.page);

  async function fetchLogs(resetPage = false) {
    if (!logViewerTask.value) return;
    if (resetPage) logFilter.value.page = 0;
    logViewerLoading.value = true;
    try {
      const offset = logFilter.value.page * logFilter.value.limit;
      const result = await tasks.logs(logViewerTask.value.id, {
        date: logFilter.value.date || undefined,
        limit: logFilter.value.limit,
        offset,
      });
      logViewerEntries.value = result.entries;
      logTotal.value = result.total;
      await nextTick();
      if (logViewerBody.value) logViewerBody.value.scrollTop = 0;
    } finally {
      logViewerLoading.value = false;
    }
  }

  function openLogViewer(task: TaskEntry) {
    logViewerTask.value = task;
    logFilter.value = { date: "", limit: 100, page: 0 };
    logViewerEntries.value = [];
    logTotal.value = 0;
    void fetchLogs();
  }

  function closeLogViewer() {
    logViewerTask.value = null;
  }

  function prevPage() {
    if (logFilter.value.page > 0) {
      logFilter.value.page--;
      void fetchLogs();
    }
  }

  function nextPage() {
    if (logFilter.value.page + 1 < logTotalPages.value) {
      logFilter.value.page++;
      void fetchLogs();
    }
  }

  let unsubWs: (() => void) | null = null;

  onMounted(() => {
    load().catch((error) => {
      console.error(error);
      alert(`任务页面加载失败: ${(error as Error).message}`);
    });

    unsubWs = ws.onMessage((msg) => {
      if (msg.type === "TaskStarted") {
        load().catch(console.error);
        return;
      }
      if (msg.type === "TaskFinished") {
        load().catch(console.error);
        return;
      }
      if (msg.type === "TaskStopped") {
        load().catch(console.error);
      }
    });
  });

  onUnmounted(() => {
    unsubWs?.();
  });

  return {
    taskItems,
    pageSize,
    listPage,
    listPageInput,
    selectedTaskIds,
    listTotalPages,
    pagedTaskItems,
    allPagedTasksSelected,
    somePagedTasksSelected,
    goToListPage,
    jumpToListPage,
    load,
    stopTask,
    rerunTask,
    clearFinished,
    toggleTaskSelection,
    toggleCurrentPageSelection,
    deleteSingleTask,
    deleteSelectedTasks,
    formatTime,
    formatTaskDuration,
    statusTone,
    logViewerTask,
    logViewerEntries,
    logViewerLoading,
    logTotal,
    logViewerBody,
    logFilter,
    logTotalPages,
    currentPage,
    fetchLogs,
    openLogViewer,
    closeLogViewer,
    logLevelClass,
    prevPage,
    nextPage,
  };
}

export type UseTasksReturn = ReturnType<typeof useTasks>;
