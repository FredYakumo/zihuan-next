<template>
  <section class="page">
    <AdminPageHeader title="计划任务" />
    <t-tabs v-model="activeTab">
      <t-tab-panel value="catalog" label="可用任务">
        <div class="task-toolbar">
          <span>{{ catalog === null ? '已注册任务：—' : `已注册任务：${catalog.jobs.length}` }}</span>
          <div class="task-toolbar-actions">
            <t-button variant="outline" size="small" :disabled="catalogLoading" @click="openCreate">新建任务</t-button>
            <t-tooltip content="重新扫描 scheduled_jobs 并重载注册表"><t-button variant="outline" size="small" :loading="reloadLoading" @click="reloadJobs">重新加载</t-button></t-tooltip>
            <t-tooltip content="刷新可用任务"><t-button shape="square" variant="text" aria-label="刷新可用任务" :loading="catalogLoading" @click="loadCatalog"><RefreshIcon /></t-button></t-tooltip>
          </div>
        </div>
        <t-alert v-if="catalogError" theme="error" :message="catalogError">
          <template #operation><t-button variant="text" :disabled="catalogLoading" @click="loadCatalog">重试</t-button></template>
        </t-alert>
        <div v-if="catalog !== null || catalogLoading" class="task-table-scroll">
          <t-table :data="jobs" :columns="jobColumns" row-key="task_name" :loading="catalogLoading" bordered size="small" empty="暂无已注册任务" class="catalog-table">
            <template #builtin="{ row }"><t-tag variant="light" :theme="row.builtin ? 'primary' : 'default'">{{ row.builtin ? '内置' : '自定义' }}</t-tag></template>
            <template #actions="{ row }">
              <t-button variant="text" size="small" @click="openEditor(row)">编辑</t-button>
              <t-tooltip v-if="row.builtin" content="内置脚本由应用提供，不能删除"><span><t-button variant="text" theme="danger" size="small" disabled>删除</t-button></span></t-tooltip>
              <t-popconfirm v-else content="确认删除该任务脚本吗？" @confirm="removeScript(row)"><t-button variant="text" theme="danger" size="small">删除</t-button></t-popconfirm>
            </template>
          </t-table>
        </div>
      </t-tab-panel>
      <t-tab-panel value="pending" label="待执行">
      <div class="task-toolbar"><span>待执行</span><t-tooltip content="刷新待执行任务"><t-button shape="square" variant="text" aria-label="刷新待执行任务" :loading="pendingLoading" @click="initPending"><RefreshIcon /></t-button></t-tooltip></div>
      <p class="muted">执行记录已移入 <RouterLink to="/tasks">任务管理器</RouterLink>，在那里以「计划任务」类型的任务列出。</p>
      <t-alert v-if="pendingError" theme="error" :message="pendingError" />
      <div class="scheduled-task-filters">
        <t-select v-model="serviceId" placeholder="选择 QQ Chat Service" @change="load">
          <t-option v-for="service in qqServices" :key="service.config_id" :value="service.config_id" :label="service.name" />
        </t-select>
      </div>
      <div v-if="!serviceId" class="empty-state">请选择一个 QQ Chat Service。</div>
      <div v-else class="task-table-scroll">
      <t-table :data="items" :columns="columns" row-key="id" bordered size="small" :loading="pendingLoading" empty="暂无待执行任务" class="records-table">
        <template #time="{ row }">{{ formatTime(row.start_time) }}</template>
        <template #summary="{ row }"><span class="task-cell-ellipsis">{{ row.info_summary ?? "-" }}</span></template>
        <template #actions="{ row }"><t-button variant="text" theme="danger" size="small" @click="cancel(row.id)">取消</t-button></template>
      </t-table>
      </div>
      </t-tab-panel>
    </t-tabs>

    <t-dialog
      v-model:visible="editorVisible"
      :header="isCreating ? '新建任务脚本' : `编辑脚本 ${editorScript}`"
      :close-on-overlay-click="false"
      :confirm-btn="null"
      :cancel-btn="null"
      width="880px"
      @close="closeEditor"
    >
      <div class="script-editor">
        <div v-if="isCreating" class="script-editor-field">
          <label>脚本文件名</label>
          <t-input v-model="newScriptName" placeholder="例如 my_job.py 或 my_job.mjs" />
          <span class="script-editor-hint">只能使用 .py 或 .mjs，保存在 scheduled_jobs/ 下。</span>
        </div>
        <div v-else class="script-editor-meta">
          <span>任务名：<strong>{{ editorTaskName }}</strong></span>
          <span>脚本语言：{{ editorLanguage }}</span>
          <t-tag v-if="editorBuiltin" variant="light" theme="primary">内置</t-tag>
        </div>
        <div class="script-editor-wrap">
          <div ref="gutterRef" class="script-editor-gutter">
            <div v-for="n in lineCount" :key="n">{{ n }}</div>
          </div>
          <textarea
            ref="editorRef"
            v-model="editorContent"
            class="script-editor-area"
            spellcheck="false"
            wrap="off"
            placeholder="编写任务脚本…"
            @scroll="syncEditorScroll"
          ></textarea>
        </div>
        <t-alert v-if="editorError" theme="error" :message="editorError" />
      </div>
      <template #footer>
        <div class="script-editor-actions">
          <t-button theme="primary" :loading="saving" @click="saveScript">保存</t-button>
          <t-button variant="outline" :disabled="saving" @click="closeEditor">取消</t-button>
        </div>
      </template>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RefreshIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import type { PrimaryTableCol } from "tdesign-vue-next";
import { scheduledTasks, system } from "../../api/client";
import type { ScheduledTaskEntry, SchedulerJobStatus, SchedulerStatus } from "../../api/types";

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
</script>

<style scoped>
.page { min-width: 0; }
.task-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 0; }
.task-toolbar-actions { display: flex; align-items: center; gap: 8px; }
.scheduled-task-filters { display: flex; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.scheduled-task-filters > * { width: 260px; max-width: 100%; }
.task-table-scroll { max-width: 100%; overflow-x: auto; margin-top: 12px; }
.catalog-table { min-width: 1180px; }
.records-table { min-width: 850px; }
.task-table-scroll :deep(td) { white-space: normal; overflow-wrap: anywhere; }
.script-editor { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
.script-editor-field { display: flex; flex-direction: column; gap: 6px; }
.script-editor-field > label { font-size: 13px; color: var(--td-text-color-secondary); }
.script-editor-hint { font-size: 12px; color: var(--td-text-color-placeholder); }
.script-editor-meta { display: flex; align-items: center; gap: 16px; font-size: 13px; color: var(--td-text-color-secondary); }
.script-editor-wrap { display: grid; grid-template-columns: 48px minmax(0, 1fr); height: 420px; overflow: hidden; }
.script-editor-gutter {
  padding: 12px 8px; border: 1px solid var(--td-border-level-1-color); border-right: 0; border-radius: 6px 0 0 6px;
  background: var(--td-bg-color-container-hover); color: var(--td-text-color-placeholder); text-align: right; overflow: hidden;
  font: 13px/1.6 ui-monospace, SFMono-Regular, Consolas, monospace; user-select: none;
}
.script-editor-gutter > div { min-height: 1.6em; }
.script-editor-area {
  min-width: 0; resize: none; border: 1px solid var(--td-border-level-1-color); border-radius: 0 6px 6px 0; padding: 12px;
  background: var(--td-bg-color-container); color: var(--td-text-color-primary); outline: none; overflow: auto; white-space: pre;
  font: 13px/1.6 ui-monospace, SFMono-Regular, Consolas, monospace;
}
.script-editor-area:focus { border-color: var(--td-brand-color); }
.script-editor-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
