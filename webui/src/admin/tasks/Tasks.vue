<template>
  <section class="page tasks-page">
    <AdminPageHeader title="任务管理器" />

    <t-card title="任务列表" bordered header-bordered>
      <template #actions>
        <t-button variant="text" @click="load">刷新</t-button>
        <t-button theme="danger" variant="outline" :disabled="selectedTaskIds.size === 0" @click="deleteSelectedTasks">
          删除选中
        </t-button>
        <t-button variant="outline" @click="clearFinished">清理已结束任务</t-button>
      </template>
      <p class="muted">共 {{ taskItems.length }} 个。</p>

      <div v-if="taskItems.length === 0" class="empty-state">还没有任务。</div>
      <template v-else>
        <t-table
          row-key="id"
          bordered
          size="small"
          :data="pagedTaskItems"
          :columns="columns"
          :selected-row-keys="selectedRowKeys"
          @select-change="onSelectChange"
        >
          <template #task="{ row }">
            <div class="task-cell-title">
              <strong>{{ row.graph_name }}</strong>
              <t-tag variant="light" :theme="row.task_type !== 'node_graph' ? 'primary' : 'default'">
                {{ row.task_type === "workspace_chat" ? "Workspace 对话" : row.task_type === "agent_service" ? "RoleService 工具" : "节点图" }}
              </t-tag>
            </div>
            <div class="mono task-cell-id">{{ row.id }}</div>
          </template>
          <template #start_time="{ row }">{{ formatTime(row.start_time) }}</template>
          <template #duration="{ row }">{{ formatTaskDuration(row) }}</template>
          <template #file_path="{ row }">
            <span class="mono task-cell-ellipsis" :title="row.file_path ?? undefined">{{ row.file_path ?? (row.chat_session_id ?? "-") }}</span>
          </template>
          <template #summary="{ row }">
            <span class="task-cell-ellipsis">{{ row.result_summary ?? row.error_message ?? "-" }}</span>
          </template>
          <template #status="{ row }">
            <t-tag variant="light" :theme="statusTagTheme(row.status)">{{ row.status }}</t-tag>
          </template>
          <template #actions="{ row }">
            <t-button variant="text" size="small" :disabled="!row.is_running || row.task_type === 'agent_service'" @click="stopTask(row.id)">
              停止
            </t-button>
            <t-button variant="text" size="small" :disabled="!row.can_rerun" @click="rerunTask(row.id)">重跑</t-button>
            <t-button variant="text" size="small" @click="openLogViewer(row)">查看日志</t-button>
            <t-button v-if="row.task_type === 'workspace_chat'" variant="text" size="small" :disabled="!row.chat_session_id" @click="openChatSession(row)">
              打开对话
            </t-button>
            <t-button v-else variant="text" size="small" :disabled="row.task_type !== 'agent_service' || !row.file_path" @click="openTaskGraph(row.id)">
              打开节点图
            </t-button>
            <t-button variant="text" theme="danger" size="small" @click="deleteSingleTask(row)">删除</t-button>
          </template>
        </t-table>

        <t-pagination
          class="tasks-pagination-bar"
          :total="taskItems.length"
          :current="listPage"
          :page-size="pageSize"
          :page-size-options="[10, 20, 50, 100]"
          show-jumper
          @change="onPaginationChange"
        />
      </template>
    </t-card>

    <t-dialog
      :visible="!!logViewerTask"
      :header="logViewerTask ? `日志 — ${logViewerTask.graph_name}` : ''"
      width="min(85vw, 1400px)"
      attach="body"
      @close="closeLogViewer"
    >
      <template #body>
        <div class="log-viewer-controls">
          <label class="log-viewer-label">
            日期
            <input v-model="logFilter.date" type="date" class="log-viewer-input" @change="fetchLogs(true)" />
          </label>
          <label class="log-viewer-label">
            每页条数
            <t-select v-model="logFilter.limit" class="log-viewer-select" @change="fetchLogs(true)">
              <t-option :value="50" label="50" />
              <t-option :value="100" label="100" />
              <t-option :value="200" label="200" />
              <t-option :value="500" label="500" />
            </t-select>
          </label>
          <div class="log-viewer-pagination">
            <span class="muted" style="font-size: 13px">第 {{ currentPage + 1 }} / {{ logTotalPages }} 页（共 {{ logTotal }} 条）</span>
            <t-button variant="text" size="small" :disabled="currentPage === 0" @click="prevPage">
              <ChevronLeftIcon />上一页
            </t-button>
            <t-button variant="text" size="small" :disabled="currentPage + 1 >= logTotalPages" @click="nextPage">
              下一页<ChevronRightIcon />
            </t-button>
          </div>
          <t-button variant="text" size="small" style="margin-left: auto" @click="fetchLogs(true)">刷新</t-button>
        </div>

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
      </template>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { ChevronLeftIcon, ChevronRightIcon } from "tdesign-icons-vue-next";
import type { PrimaryTableCol } from "tdesign-vue-next";

import type { TaskEntry } from "../../api/client";
import { useTasks } from "./useTasks";
import { statusTagTheme } from "../model";

const {
  taskItems,
  pageSize,
  listPage,
  selectedTaskIds,
  pagedTaskItems,
  goToListPage,
  load,
  stopTask,
  rerunTask,
  clearFinished,
  deleteSingleTask,
  deleteSelectedTasks,
  formatTime,
  formatTaskDuration,
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

const selectedRowKeys = computed(() => [...selectedTaskIds.value]);

function onSelectChange(keys: Array<string | number>) {
  selectedTaskIds.value = new Set(keys as string[]);
}

function onPaginationChange(pageInfo: { current: number; pageSize: number }) {
  pageSize.value = pageInfo.pageSize;
  goToListPage(pageInfo.current);
}

function openTaskGraph(taskId: string) {
  window.open(`/editor?task_graph=${encodeURIComponent(taskId)}`, "_blank", "noopener");
}

function openChatSession(task: TaskEntry) {
  if (!task.chat_session_id) return;
  const query = new URLSearchParams({ session_id: task.chat_session_id, agent_id: task.graph_session_id });
  window.open(`/chat?${query.toString()}`, "_blank", "noopener");
}

const columns: PrimaryTableCol<TaskEntry>[] = [
  { colKey: "row-select", type: "multiple", width: 48 },
  { colKey: "task", title: "任务" },
  { colKey: "start_time", title: "开始时间" },
  { colKey: "duration", title: "耗时" },
  { colKey: "file_path", title: "来源" },
  { colKey: "summary", title: "摘要" },
  { colKey: "status", title: "状态" },
  { colKey: "actions", title: "操作", width: 300 },
];
</script>

<style scoped lang="scss">
@use "./tasks" as *;
</style>
