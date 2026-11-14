<template>
  <section class="page model-weights-page">
    <AdminPageHeader title="模型权重文件" />

    <p v-if="error" class="model-weights-error" role="alert">{{ error }}</p>
    <p v-if="importError" class="model-weights-error" role="alert">{{ importError }}</p>

    <t-card
      v-for="group in groups"
      :key="group.kind"
      :title="group.label"
      bordered
      header-bordered
    >
      <template #actions>
        <t-space size="small">
          <t-button variant="text" @click="load">刷新</t-button>
          <t-button variant="outline" @click="openLocalDir(group.kind)">打开本地目录</t-button>
          <t-button
            variant="outline"
            :loading="importLoading === group.kind"
            @click="importWeights(group.kind)"
          >
            从本地目录导入
          </t-button>
          <t-button
            v-if="group.kind === 'text_embedding' || group.kind === 'llm'"
            theme="primary"
            @click="openDownload(group.kind)"
          >
            下载模型
          </t-button>
        </t-space>
      </template>

      <t-table
        :data="group.models"
        row-key="name"
        :columns="columns"
        :loading="loading"
        :pagination="false"
        table-layout="fixed"
      >
        <template #status="{ row }">
          <t-tag variant="light" :theme="row.valid ? 'success' : 'warning'">
            {{ row.valid ? "就绪" : "不完整" }}
          </t-tag>
          <span v-if="row.detail" class="model-weights-detail" :title="row.detail">{{ row.detail }}</span>
        </template>
        <template #size="{ row }">{{ formatBytes(row.size_bytes) }}</template>
        <template #actions="{ row }">
          <t-space size="small">
            <t-button variant="text" size="small" @click="openRename(group.kind, row.name)">重命名</t-button>
            <t-popconfirm
              :content="`确认删除模型目录「${row.name}」吗？该操作不可恢复。`"
              @confirm="removeWeights(group.kind, row.name)"
            >
              <t-button variant="text" theme="danger" size="small">删除</t-button>
            </t-popconfirm>
          </t-space>
        </template>
        <template #empty><div class="model-weights-empty">该目录下暂无模型权重文件。</div></template>
      </t-table>
    </t-card>

    <t-card v-if="jobs.length" title="下载任务" bordered header-bordered>
      <div class="model-weights-jobs">
        <div v-for="job in jobs" :key="job.id" class="model-weights-job">
          <div class="model-weights-job-head">
            <strong>{{ job.repo_id }}</strong>
            <t-space size="small">
              <t-button
                v-if="job.status === 'queued' || job.status === 'downloading'"
                variant="text"
                theme="danger"
                size="small"
                @click="cancelJob(job.id)"
              >
                终止
              </t-button>
              <t-tag
                variant="light"
                :theme="job.status === 'completed' ? 'success' : job.status === 'failed' ? 'danger' : 'warning'"
              >
                {{ jobStatus(job) }}
              </t-tag>
            </t-space>
          </div>
          <t-progress
            v-if="job.status === 'downloading' || job.status === 'queued'"
            :percentage="jobPercent(job)"
            :status="job.status === 'queued' ? 'warning' : 'active'"
          />
          <p v-if="job.current_file" class="model-weights-job-meta">正在下载：{{ job.current_file }}</p>
          <p v-if="job.error" class="model-weights-job-error">{{ job.error }}</p>
        </div>
      </div>
    </t-card>

    <ModelDownloadDialog
      v-model:visible="downloadDialogVisible"
      :kind="downloadKind"
      @completed="onDownloadCompleted"
    />

    <t-dialog
      v-model:visible="renameDialogVisible"
      :header="`重命名模型目录${renameTarget ? `「${renameTarget.name}」` : ''}`"
      width="460px"
      :confirm-btn="{ content: '保存', loading: renameSaving }"
      @confirm="confirmRename"
      @close="closeRename"
    >
      <t-input v-model="renameName" placeholder="新的目录名" />
      <p v-if="renameError" class="model-weights-error" role="alert">{{ renameError }}</p>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ModelDownloadDialog from "../components/ModelDownloadDialog.vue";
import { useModelWeights } from "./useModelWeights";

const {
  groups,
  loading,
  error,
  columns,
  formatBytes,
  load,
  importLoading,
  importError,
  importWeights,
  downloadDialogVisible,
  downloadKind,
  openDownload,
  jobs,
  cancelJob,
  onDownloadCompleted,
  renameDialogVisible,
  renameTarget,
  renameName,
  renameSaving,
  renameError,
  openRename,
  closeRename,
  confirmRename,
  removeWeights,
} = useModelWeights();

function jobPercent(job: { downloaded_bytes: number; total_bytes: number }): number {
  if (job.total_bytes <= 0) return 0;
  return Math.min(100, Math.round((job.downloaded_bytes / job.total_bytes) * 100));
}

function jobStatus(job: { status: string }): string {
  if (job.status === "queued") return "排队中";
  if (job.status === "downloading") return "下载中";
  if (job.status === "completed") return "下载完成";
  if (job.status === "cancelled") return "已终止";
  return "下载失败";
}
</script>

<style scoped lang="scss">
.model-weights-error {
  margin: 0 0 12px;
  font-size: 13px;
  color: var(--td-error-color);
}

.model-weights-detail {
  margin-left: 8px;
  font-size: 12px;
  opacity: 0.7;
}

.model-weights-empty {
  padding: 16px 0;
  font-size: 13px;
  opacity: 0.65;
}

.model-weights-jobs {
  display: grid;
  gap: 12px;
}

.model-weights-job {
  display: grid;
  gap: 6px;
  padding: 10px 12px;
  border: 1px solid var(--admin-border, rgba(255, 255, 255, 0.12));
  border-radius: 6px;
}

.model-weights-job-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 13px;
}

.model-weights-job-meta {
  margin: 0;
  font-size: 12px;
  opacity: 0.7;
  word-break: break-all;
}

.model-weights-job-error {
  margin: 0;
  font-size: 12px;
  color: var(--td-error-color);
}
</style>
