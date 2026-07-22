<template>
  <section class="page tasks-page">
    <div class="page-hero">
      <h2>任务管理器</h2>
    </div>

    <section class="panel">
      <div class="split-header">
        <div>
          <h3>任务列表</h3>
          <p class="muted">共 {{ taskItems.length }} 个。</p>
        </div>
        <div class="inline-actions">
          <button class="btn ghost" @click="load">刷新</button>
          <button class="btn danger" :disabled="selectedTaskIds.size === 0" @click="deleteSelectedTasks">
            删除选中
          </button>
          <button class="btn" @click="clearFinished">清理已结束任务</button>
        </div>
      </div>
      <div class="tasks-list-shell tasks-pagination">
        <label class="tasks-pagination-label">
          每页条数
          <select v-model.number="pageSize" class="field-input tasks-pagination-select">
            <option :value="10">10</option>
            <option :value="20">20</option>
            <option :value="50">50</option>
            <option :value="100">100</option>
          </select>
        </label>
        <span class="muted tasks-pagination-status">
          第 {{ listPage }} / {{ listTotalPages }} 页
          <span>共 {{ taskItems.length }} 条</span>
        </span>
        <div class="tasks-pagination-actions">
          <button class="btn ghost" :disabled="listPage <= 1" @click="goToListPage(1)">首页</button>
          <button class="btn ghost" :disabled="listPage <= 1" @click="goToListPage(listPage - 1)">上一页</button>
          <label class="tasks-pagination-label">
            跳转
            <input
              v-model.number="listPageInput"
              type="number"
              min="1"
              :max="listTotalPages"
              class="field-input tasks-pagination-input"
              @keydown.enter.prevent="jumpToListPage"
            />
          </label>
          <button class="btn ghost" @click="jumpToListPage">前往</button>
          <button class="btn ghost" :disabled="listPage >= listTotalPages" @click="goToListPage(listPage + 1)">下一页</button>
          <button class="btn ghost" :disabled="listPage >= listTotalPages" @click="goToListPage(listTotalPages)">末页</button>
        </div>
      </div>
      <div class="tasks-table tasks-list-shell" style="margin-top: 16px;">
        <div v-if="taskItems.length > 0" class="tasks-table-head">
          <span>
            <input
              type="checkbox"
              :checked="allPagedTasksSelected"
              :indeterminate="somePagedTasksSelected && !allPagedTasksSelected"
              aria-label="选择当前页任务"
              @change="toggleCurrentPageSelection"
            />
          </span>
          <span>任务</span>
          <span>开始时间</span>
          <span>耗时</span>
          <span>来源</span>
          <span>摘要</span>
          <span>状态</span>
          <span>操作</span>
        </div>
        <div v-if="taskItems.length === 0" class="empty-state">还没有任务。</div>
        <article
          v-for="task in pagedTaskItems"
          :key="task.id"
          class="record task-row-card"
        >
          <div class="task-row-check">
            <input
              type="checkbox"
              :checked="selectedTaskIds.has(task.id)"
              :aria-label="`选择任务 ${task.graph_name}`"
              @change="toggleTaskSelection(task.id)"
            />
          </div>
          <div class="task-row-main">
            <div class="task-row-title">
              <h4>{{ task.graph_name }}</h4>
              <div class="task-row-badges">
                <span class="badge task-type-badge" :class="task.task_type === 'agent_service' ? 'task-type-agent' : 'task-type-graph'">
                  {{ task.task_type === "agent_service" ? "Agent 响应" : "节点图" }}
                </span>
              </div>
            </div>
            <div class="task-row-id mono">{{ task.id }}</div>
          </div>
          <div class="task-row-meta">
            <span class="task-row-label">开始时间</span>
            <span>{{ formatTime(task.start_time) }}</span>
          </div>
          <div class="task-row-meta">
            <span class="task-row-label">耗时</span>
            <span>{{ formatTaskDuration(task) }}</span>
          </div>
          <div class="task-row-meta">
            <span class="task-row-label">来源</span>
            <span class="mono task-row-ellipsis">{{ task.file_path ?? "-" }}</span>
          </div>
          <div class="task-row-summary">
            <span class="task-row-label">摘要</span>
            <span class="task-row-ellipsis">{{ task.result_summary ?? task.error_message ?? "-" }}</span>
          </div>
          <div class="task-row-status">
            <div style="display:flex;align-items:center;gap:8px;">
              <span class="badge" :class="statusTone(task.status)">{{ task.status }}</span>
            </div>
          </div>
          <div class="panel-actions task-row-actions">
            <button
              class="btn"
              :disabled="!task.is_running || task.task_type === 'agent_service'"
              @click="stopTask(task.id)"
            >
              停止
            </button>
            <button class="btn" :disabled="!task.can_rerun" @click="rerunTask(task.id)">重跑</button>
            <button class="btn ghost" @click="openLogViewer(task)">查看日志</button>
            <button class="btn danger" @click="deleteSingleTask(task)">删除</button>
          </div>
        </article>
      </div>
    </section>

    <!-- Log viewer dialog -->
    <div v-if="logViewerTask" class="connection-picker-backdrop">
      <div class="connection-picker-dialog log-viewer-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>日志 — {{ logViewerTask.graph_name }}</h3>
          <button class="task-terminal-btn" aria-label="关闭" @click="closeLogViewer"><CloseIcon /></button>
        </div>

        <!-- Controls -->
        <div class="log-viewer-controls">
          <label class="log-viewer-label">
            日期
            <input
              v-model="logFilter.date"
              type="date"
              class="log-viewer-input"
              @change="fetchLogs(true)"
            />
          </label>
          <label class="log-viewer-label">
            每页条数
            <select v-model="logFilter.limit" class="log-viewer-input" @change="fetchLogs(true)">
              <option :value="50">50</option>
              <option :value="100">100</option>
              <option :value="200">200</option>
              <option :value="500">500</option>
            </select>
          </label>
          <div class="log-viewer-pagination">
            <span class="muted" style="font-size:13px;">第 {{ currentPage + 1 }} / {{ logTotalPages }} 页（共 {{ logTotal }} 条）</span>
            <button class="task-terminal-btn" :disabled="currentPage === 0" @click="prevPage"><ChevronLeftIcon /> 上一页</button>
            <button class="task-terminal-btn" :disabled="currentPage + 1 >= logTotalPages" @click="nextPage">下一页 <ChevronRightIcon /></button>
          </div>
          <button class="task-terminal-btn" style="margin-left:auto;" @click="fetchLogs(true)">刷新</button>
        </div>

        <!-- Log body -->
        <div ref="logViewerBody" class="task-terminal-body log-viewer-body">
          <div v-if="logViewerLoading" class="task-terminal-hint">加载中…</div>
          <div v-else-if="logViewerEntries.length === 0" class="task-terminal-hint">暂无日志。</div>
          <div
            v-for="(entry, index) in logViewerEntries"
            :key="`${entry.timestamp}-${index}`"
            class="task-terminal-line"
            :class="logLevelClass(entry.level)"
          >
            <span class="task-terminal-ts">{{ entry.timestamp }}</span>
            <span class="task-terminal-level">{{ entry.level }}</span>
            <span class="task-terminal-msg">{{ entry.message }}</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { ChevronLeftIcon, ChevronRightIcon, CloseIcon } from "tdesign-icons-vue-next";

import { useTasks } from "../composables/useTasks";

const {
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
} = useTasks();
</script>

<style scoped lang="scss">
@use "../styles/tasks" as *;
</style>
