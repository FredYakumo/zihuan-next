<template>
  <section class="page settings-page">
    <AdminPageHeader title="设置" />

    <div class="grid-2">
      <t-card title="主题" bordered header-bordered>
        <template #actions>
          <t-tag variant="light">{{ currentThemeSchemaLabel }}</t-tag>
        </template>
        <p class="muted">调整界面配色方案。</p>

        <div class="settings-theme-body">
          <div class="field">
            <label>当前方案</label>
            <t-select :value="selectedTheme" @change="handleThemeChange">
              <t-option value="system" label="跟随系统" />
              <t-option
                v-for="theme in themeOptions"
                :key="theme.name"
                :value="theme.name"
                :label="theme.display_name"
              />
            </t-select>
          </div>

          <div class="theme-preview-card" :style="themePreviewStyle">
            <div class="theme-preview-toolbar" :style="themePreviewToolbarStyle">
              <span>{{ currentThemeLabel }}</span>
              <span class="theme-preview-muted">{{ currentThemeSchemaLabel }}</span>
            </div>
            <div class="theme-preview-body">
              <div class="theme-preview-chip" :style="themeAccentStyle">Accent</div>
              <div class="theme-preview-lines">
                <span></span>
                <span></span>
              </div>
            </div>
          </div>
        </div>
      </t-card>

      <t-card title="日志提示" bordered header-bordered>
        <p class="muted">在导航栏"日志"旁显示未读错误数量红点。</p>
        <t-checkbox :checked="logErrorBadgeEnabled" @change="handleLogErrorBadgeToggle">显示错误提示</t-checkbox>
      </t-card>

      <t-card title="模型 HTTP 服务" bordered header-bordered>
        <template #actions>
          <t-switch :value="modelHttpEnabled" :loading="modelHttpSaving" @change="setModelHttpEnabled" />
        </template>
        <p class="muted">启用主服务的 OpenAI 兼容接口：<code>/v1/chat/completions</code></p>
        <t-button :disabled="!modelHttpEnabled" @click="modelHttpDialogVisible = true">模型配置</t-button>
      </t-card>
    </div>

    <t-card title="模型 HTTP API Keys" bordered header-bordered>
      <template #actions><t-button theme="primary" @click="handleCreateModelHttpApiKey">创建 API Key</t-button></template>
      <p class="muted">Key 仅在创建时显示明文；分组字段已预留给后续授权功能。</p>
      <t-table :data="modelHttpApiKeys" :columns="modelHttpApiKeyColumns" row-key="id" :pagination="false" size="small">
        <template #enabled="{ row }"><t-switch :value="row.enabled" @change="updateModelHttpApiKey(row, { enabled: $event })" /></template>
        <template #actions="{ row }"><t-popconfirm content="确认删除此 API Key？" @confirm="deleteModelHttpApiKey(row.id)"><t-button variant="text" theme="danger">删除</t-button></t-popconfirm></template>
      </t-table>
    </t-card>

    <t-dialog v-model:visible="modelHttpDialogVisible" header="公开模型配置" width="680px" :confirm-btn="{ content: '保存' }" @confirm="saveModelHttpSettings">
      <p class="muted">请求中的 <code>model</code> 使用模型配置的底层 Model Name；同名模型不能同时公开。</p>
      <div class="settings-backup-actions"><t-checkbox :checked="allPublicModelsSelected" @change="toggleAllPublicModels">全选</t-checkbox></div>
      <t-checkbox-group v-model="publicModelConfigIds">
        <div v-for="model in enabledChatModels" :key="model.config_id" class="settings-path-row">
          <t-checkbox :value="model.config_id">{{ model.name }}（{{ model.model_name }}）</t-checkbox>
        </div>
      </t-checkbox-group>
    </t-dialog>

    <t-dialog v-model:visible="modelHttpSecretDialogVisible" header="请立即保存 API Key" :footer="false">
      <p>此 Key 之后无法再次查看。</p>
      <t-input :value="newModelHttpSecret" readonly />
      <div class="settings-backup-actions"><t-button theme="primary" @click="copyModelHttpSecret">复制</t-button></div>
    </t-dialog>

    <t-card title="Python 运行时" bordered header-bordered>
      <template #actions>
        <t-button variant="text" :disabled="pythonRuntimeLoading" @click="reloadPythonRuntime">
          重新检查
        </t-button>
      </template>
      <p class="muted">Python 工具默认使用的解释器，可由单个工具覆盖。</p>

      <div class="settings-python-body">
        <div v-if="pythonRuntimeLoading" class="settings-python-pending">
          <t-loading size="small" />
          <span>检测中</span>
        </div>
        <div v-else-if="pythonRuntime" class="settings-python-status">
          <t-tag variant="light" :theme="pythonRuntime.available ? 'success' : 'warning'">
            {{ pythonRuntime.available ? "可用" : "不可用" }}
          </t-tag>
          <span v-if="pythonRuntime.executable_path" class="settings-path-label">Python 路径</span>
          <code v-if="pythonRuntime.executable_path" class="settings-path-value">{{ pythonRuntime.executable_path }}</code>
          <span v-if="pythonRuntime.version" class="muted">{{ pythonRuntime.version }}</span>
          <span v-if="pythonRuntime.diagnostic" class="settings-python-error">{{ pythonRuntime.diagnostic }}</span>
        </div>
        <div v-else class="settings-python-pending">
          <ErrorCircleIcon />
          <span>暂未检测到</span>
        </div>

        <div class="settings-backup-actions">
          <t-button theme="primary" :disabled="pythonRuntimeChanging" @click="changePythonRuntime">
            {{ pythonRuntimeChanging ? "选择中…" : "更改" }}
          </t-button>
          <span v-if="pythonRuntimeError" class="settings-python-error">{{ pythonRuntimeError }}</span>
        </div>
      </div>
    </t-card>

    <t-card title="数据目录" bordered header-bordered>
      <template #actions>
        <t-button variant="text" @click="reloadStorageInfo">刷新</t-button>
      </template>
      <p class="muted">应用运行时产生的数据的存储位置。</p>

      <div v-if="storageInfo" class="settings-storage-body">
        <div class="settings-path-row settings-path-row--root">
          <span class="settings-path-label">应用数据目录</span>
          <code class="settings-path-value">{{ storageInfo.data_dir }}</code>
        </div>
        <div v-for="entry in storageInfo.storage_entries" :key="entry.path" class="settings-path-row">
          <span class="settings-path-label">{{ entry.label }}</span>
          <code class="settings-path-value">{{ entry.path }}</code>
          <t-tag variant="light" :theme="entry.exists ? 'success' : 'warning'">
            {{ entry.exists ? "存在" : "未创建" }}
          </t-tag>
        </div>
      </div>
      <div v-else-if="storageLoading" class="empty-state">加载中…</div>
      <div v-else-if="storageError" class="empty-state">{{ storageError }}</div>
    </t-card>

    <t-card title="配置备份" bordered header-bordered>
      <template #actions>
        <div class="settings-backup-actions">
          <t-button variant="outline" @click="handleExportConfig">导出配置</t-button>
          <t-button theme="primary" :disabled="restoreLoading" @click="triggerRestorePicker">
            {{ restoreLoading ? "恢复中…" : "恢复配置" }}
          </t-button>
          <input
            ref="restoreFileInput"
            type="file"
            accept=".zip"
            class="settings-backup-file-input"
            @change="handleRestoreFileChange"
          />
        </div>
      </template>

      <div v-if="restoreSuccess" class="settings-backup-feedback settings-backup-feedback--ok">
        配置已成功恢复，请重启服务以使新配置生效。
      </div>
      <div v-if="restoreError" class="settings-backup-feedback settings-backup-feedback--err">
        {{ restoreError }}
      </div>
      <p v-if="!restoreSuccess && !restoreError" class="muted">导出当前配置为压缩包，或从备份文件恢复。</p>
    </t-card>

    <t-card
      v-for="group in modelGroups"
      :key="group.label"
      :title="`本地模型 — ${group.label}`"
      bordered
      header-bordered
    >
      <p class="muted">{{ group.dir }}</p>

      <div v-if="group.models.length === 0" class="empty-state">该目录下暂无可用模型。</div>
      <div v-else class="settings-model-list">
        <article v-for="model in group.models" :key="model.name" class="settings-model-card">
          <div class="settings-model-header">
            <strong>{{ model.name }}</strong>
            <t-tag variant="light" :theme="model.valid ? 'success' : 'warning'">
              {{ model.valid ? "就绪" : "不完整" }}
            </t-tag>
          </div>
          <code class="settings-path-value">{{ model.path }}</code>
          <div v-if="model.size_bytes != null" class="settings-model-meta">
            <span class="muted">大小：{{ formatBytes(model.size_bytes) }}</span>
          </div>
        </article>
      </div>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import { ErrorCircleIcon } from "tdesign-icons-vue-next";
import { ref } from "vue";

import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useSettings } from "../composables/useSettings";

const {
  themeOptions,
  selectedTheme,
  currentThemeLabel,
  currentThemeSchemaLabel,
  themePreviewStyle,
  themePreviewToolbarStyle,
  themeAccentStyle,
  handleThemeChange,
  storageInfo,
  storageLoading,
  storageError,
  modelGroups,
  reloadStorageInfo,
  formatBytes,
  restoreFileInput,
  restoreLoading,
  restoreError,
  restoreSuccess,
  triggerRestorePicker,
  handleRestoreFileChange,
  handleExportConfig,
  pythonRuntime,
  pythonRuntimeLoading,
  pythonRuntimeChanging,
  pythonRuntimeError,
  reloadPythonRuntime,
  changePythonRuntime,
  logErrorBadgeEnabled,
  handleLogErrorBadgeToggle,
  modelHttpEnabled,
  modelHttpSaving,
  publicModelConfigIds,
  modelHttpApiKeys,
  enabledChatModels,
  allPublicModelsSelected,
  newModelHttpSecret,
  setModelHttpEnabled,
  saveModelHttpSettings,
  toggleAllPublicModels,
  createModelHttpApiKey,
  updateModelHttpApiKey,
  deleteModelHttpApiKey,
  copyModelHttpSecret,
} = useSettings();

const modelHttpDialogVisible = ref(false);
const modelHttpSecretDialogVisible = ref(false);
const modelHttpApiKeyColumns = [
  { colKey: "name", title: "名称" },
  { colKey: "secret_prefix", title: "Key 前缀" },
  { colKey: "created_at", title: "创建时间" },
  { colKey: "expires_at", title: "过期时间" },
  { colKey: "group", title: "分组" },
  { colKey: "enabled", title: "启用", width: 80 },
  { colKey: "actions", title: "操作", width: 80 },
];

async function handleCreateModelHttpApiKey() {
  await createModelHttpApiKey();
  if (newModelHttpSecret.value) modelHttpSecretDialogVisible.value = true;
}
</script>

<style scoped lang="scss">
@use "../styles/settings" as *;
</style>
