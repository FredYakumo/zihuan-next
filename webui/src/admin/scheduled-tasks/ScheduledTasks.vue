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
import { RefreshIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useScheduledTasksPage } from "./ScheduledTasks";

const {
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
} = useScheduledTasksPage();
</script>

<style scoped lang="scss">
@use "./scheduled-tasks" as *;
</style>
