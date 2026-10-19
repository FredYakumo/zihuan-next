import { computed, onMounted, ref, watch } from "vue";
import type { PrimaryTableCol } from "tdesign-vue-next";

import { scheduledTasks, system } from "../../api/client";
import type { ScheduledTaskEntry, SchedulerJobStatus, SchedulerStatus } from "../../api/types";

export function useScheduledTasksPage() {
  const activeTab = ref("catalog");
  const catalog = ref<SchedulerStatus | null>(null);
  const catalogLoading = ref(false);
  const catalogError = ref("");
  const jobs = computed(() => [...(catalog.value?.jobs ?? [])].sort((a, b) => a.task_name.localeCompare(b.task_name)));
  const pendingLoading = ref(false);
  const pendingError = ref("");
  let servicesLoaded = false;
  let pendingVisited = false;
  let pendingRequest = 0;
  function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }
  async function loadCatalog() {
    if (catalogLoading.value) return;
    catalogLoading.value = true;
    catalogError.value = "";
    try { catalog.value = await scheduledTasks.catalog(); }
    catch (error) { catalogError.value = `加载可用任务失败：${errorText(error)}`; }
    finally { catalogLoading.value = false; }
  }

  const editorVisible = ref(false);
  const editorScript = ref("");
  const editorContent = ref("");
  const editorTaskName = ref("");
  const editorLanguage = ref("");
  const editorBuiltin = ref(false);
  const editorError = ref("");
  const isCreating = ref(false);
  const newScriptName = ref("");
  const saving = ref(false);
  const reloadLoading = ref(false);
  const editorRef = ref<HTMLTextAreaElement | null>(null);
  const gutterRef = ref<HTMLDivElement | null>(null);
  const lineCount = computed(() => (editorContent.value ? editorContent.value.split("\n").length : 1));
  function syncEditorScroll() {
    const editor = editorRef.value;
    const gutter = gutterRef.value;
    if (editor && gutter) gutter.scrollTop = editor.scrollTop;
  }
  async function reloadJobs() {
    if (reloadLoading.value) return;
    reloadLoading.value = true;
    catalogError.value = "";
    try { await scheduledTasks.reload(); await loadCatalog(); }
    catch (error) { catalogError.value = `重新加载失败：${errorText(error)}`; }
    finally { reloadLoading.value = false; }
  }
  async function openEditor(row: SchedulerJobStatus) {
    editorError.value = "";
    isCreating.value = false;
    try {
      const result = await scheduledTasks.script(row.script);
      editorScript.value = row.script;
      editorContent.value = result.content;
      editorTaskName.value = row.task_name;
      editorLanguage.value = row.language;
      editorBuiltin.value = row.builtin;
      editorVisible.value = true;
    } catch (error) { catalogError.value = `加载脚本失败：${errorText(error)}`; }
  }
  function openCreate() {
    editorError.value = "";
    isCreating.value = true;
    newScriptName.value = "new_job.py";
    editorScript.value = "";
    editorTaskName.value = "";
    editorLanguage.value = "";
    editorBuiltin.value = false;
    applyTemplate();
    editorVisible.value = true;
  }
  // Keeps the generated skeleton in sync with the file name while it is still untouched, so the
  // derived `task_name` matches the file instead of stranding an empty one that cannot be saved.
  function applyTemplate() {
    editorContent.value = scriptTemplate(newScriptName.value, taskNameForFile(newScriptName.value));
  }
  watch(newScriptName, (_name, previous) => {
    if (!isCreating.value) return;
    if (editorContent.value === scriptTemplate(previous, taskNameForFile(previous))) applyTemplate();
  });
  function closeEditor() { editorVisible.value = false; }
  // New jobs start from a minimal valid manifest so the first save passes validation.
  function scriptTemplate(fileName: string, taskName: string) {
    if (fileName.trim().toLowerCase().endsWith(".mjs")) {
      return `export const job_manifest = {\n  task_name: ${JSON.stringify(taskName)},\n  entry: "run_job",\n  description: "",\n};\n\nexport function run_job(request, sdk) {\n  return { ok: true, result: "完成" };\n}\n`;
    }
    return `JOB_MANIFEST = {\n    "task_name": ${JSON.stringify(taskName)},\n    "entry": "run_job",\n    "description": "",\n}\n\n\ndef run_job(request):\n    return {"ok": True, "result": "完成"}\n`;
  }
  function taskNameForFile(fileName: string) {
    const base = fileName.replace(/\.(py|mjs)$/i, "");
    return base || "task";
  }
  async function saveScript() {
    editorError.value = "";
    const script = isCreating.value ? newScriptName.value.trim() : editorScript.value;
    if (!script) { editorError.value = "请填写脚本文件名。"; return; }
    if (!/\.(py|mjs)$/i.test(script)) { editorError.value = "脚本文件名必须以 .py 或 .mjs 结尾。"; return; }
    if (!editorContent.value.trim()) { editorError.value = "脚本内容不能为空。"; return; }
    saving.value = true;
    try {
      await scheduledTasks.saveScript(script, editorContent.value);
      editorVisible.value = false;
      await loadCatalog();
    } catch (error) { editorError.value = `保存失败：${errorText(error)}`; }
    finally { saving.value = false; }
  }
  async function removeScript(row: SchedulerJobStatus) {
    catalogError.value = "";
    try { await scheduledTasks.deleteScript(row.script); await loadCatalog(); }
    catch (error) { catalogError.value = `删除脚本失败：${errorText(error)}`; }
  }

  const serviceId = ref("");
  const items = ref<ScheduledTaskEntry[]>([]);
  const serviceItems = ref<Array<{ config_id: string; name: string; role_service_type: { type: string } }>>([]);
  const qqServices = computed(() => serviceItems.value.filter((service) => service.role_service_type.type === "qq_chat"));
  function formatTime(value: string) { return new Date(value).toLocaleString(); }
  async function load() {
    const requestId = ++pendingRequest;
    items.value = [];
    pendingError.value = "";
    if (!serviceId.value) { pendingLoading.value = false; return; }
    pendingLoading.value = true;
    try {
      const result = await scheduledTasks.list(serviceId.value, "pending");
      if (requestId === pendingRequest) items.value = result;
    } catch (error) {
      if (requestId === pendingRequest) pendingError.value = `加载待执行任务失败：${errorText(error)}`;
    } finally { if (requestId === pendingRequest) pendingLoading.value = false; }
  }
  async function cancel(taskId: string) {
    try { await scheduledTasks.cancel(serviceId.value, taskId); await load(); }
    catch (error) { pendingError.value = `取消任务失败：${errorText(error)}`; }
  }
  async function initPending() {
    if (!servicesLoaded) {
      pendingLoading.value = true;
      pendingError.value = "";
      try {
        serviceItems.value = await system.services.list() as typeof serviceItems.value;
        servicesLoaded = true;
        if (qqServices.value.length === 1) serviceId.value = qqServices.value[0].config_id;
      } catch (error) {
        pendingError.value = `加载服务失败：${errorText(error)}`;
        pendingLoading.value = false;
        return;
      }
    }
    await load();
  }
  watch(activeTab, (tab) => {
    if (tab === "pending" && !pendingVisited) { pendingVisited = true; void initPending(); }
  });
  onMounted(loadCatalog);
  const jobColumns: PrimaryTableCol<SchedulerJobStatus>[] = [
    { colKey: "task_name", title: "任务名称", width: 140 },
    { colKey: "description", title: "说明", width: 300 },
    { colKey: "builtin", title: "来源", width: 100 },
    { colKey: "language", title: "脚本语言", width: 120 },
    { colKey: "script", title: "脚本路径", width: 240 },
    { colKey: "entry", title: "入口", width: 160 },
    { colKey: "actions", title: "操作", width: 130 },
  ];
  const columns: PrimaryTableCol<ScheduledTaskEntry>[] = [
    { colKey: "task_name", title: "任务名称", width: 120 }, { colKey: "triggered_by", title: "触发者", width: 150 },
    { colKey: "time", title: "计划执行时间", width: 210 },
    { colKey: "summary", title: "信息摘要" }, { colKey: "actions", title: "操作", width: 80 },
  ];

  return {
    activeTab,
    catalog,
    catalogLoading,
    catalogError,
    jobs,
    pendingLoading,
    pendingError,
    editorVisible,
    editorScript,
    editorContent,
    editorTaskName,
    editorLanguage,
    editorBuiltin,
    editorError,
    isCreating,
    newScriptName,
    saving,
    reloadLoading,
    editorRef,
    gutterRef,
    lineCount,
    syncEditorScroll,
    loadCatalog,
    reloadJobs,
    openEditor,
    openCreate,
    closeEditor,
    saveScript,
    removeScript,
    serviceId,
    items,
    qqServices,
    formatTime,
    load,
    cancel,
    initPending,
    jobColumns,
    columns,
  };
}
