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

      <t-card title="启用模型 HTTP 服务" bordered header-bordered>
        <template #actions>
          <t-switch :value="modelHttpEnabled" :loading="modelHttpSaving" @change="setModelHttpEnabled" />
        </template>
        <template v-if="modelHttpEnabled">
          <div class="model-http-service-actions">
            <div class="model-http-service-endpoint">
              <code>{{ modelHttpEndpoint }}</code>
              <t-button variant="text" shape="square" title="复制地址" @click="copyModelHttpEndpoint">
                <FileCopyIcon />
              </t-button>
            </div>
            <t-button class="model-http-service-config-button" @click="modelHttpDialogVisible = true">
              模型配置
            </t-button>
          </div>
        </template>
      </t-card>
    </div>

    <t-dialog v-model:visible="modelHttpDialogVisible" header="启用模型" width="680px" :confirm-btn="{ content: '保存' }" @confirm="handleSaveModelHttpSettings">
      <div class="model-http-model-selection-header">
        <t-checkbox :checked="allPublicModelsSelected" @change="toggleAllPublicModels">全选</t-checkbox>
      </div>
      <t-checkbox-group v-model="publicModelConfigIds" class="model-http-model-selection-list">
        <t-checkbox v-for="model in enabledChatModels" :key="model.config_id" :value="model.config_id" class="model-http-model-option">
          {{ model.name }}<span v-if="model.has_duplicate_model_name">（{{ model.model_name }}）</span>
        </t-checkbox>
      </t-checkbox-group>
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

    <t-card title="Dynamic Script Runtime" bordered header-bordered>
      <template #actions>
        <t-button variant="text" :disabled="nodeRuntimeLoading" @click="reloadNodeRuntime">重新检查</t-button>
      </template>
      <p class="muted">用于执行动态脚本节点</p>
      <div class="settings-python-body">
        <div v-if="nodeRuntimeLoading" class="settings-python-pending"><t-loading size="small" /><span>检测中</span></div>
        <div v-else-if="nodeRuntime" class="settings-python-status">
          <t-tag variant="light" :theme="nodeRuntime.available ? 'success' : 'warning'">{{ nodeRuntime.available ? "可用" : "不可用" }}</t-tag>
          <span v-if="nodeRuntime.executable_path" class="settings-path-label">Node 路径</span>
          <code v-if="nodeRuntime.executable_path" class="settings-path-value">{{ nodeRuntime.executable_path }}</code>
          <span v-if="nodeRuntime.version" class="muted">{{ nodeRuntime.version }}</span>
          <span v-if="nodeRuntime.diagnostic" class="settings-python-error">{{ nodeRuntime.diagnostic }}</span>
        </div>
        <div class="settings-backup-actions">
          <t-button variant="outline" :disabled="nodeRuntimeChanging" @click="setNodeRuntime({ kind: 'project_node' })">使用项目 Node</t-button>
          <t-button theme="primary" :disabled="nodeRuntimeChanging" @click="chooseNodeRuntime">{{ nodeRuntimeChanging ? "更改中…" : "更改" }}</t-button>
          <span v-if="nodeRuntimeError" class="settings-python-error">{{ nodeRuntimeError }}</span>
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
import { ErrorCircleIcon, FileCopyIcon } from "tdesign-icons-vue-next";
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
  nodeRuntime,
  nodeRuntimeLoading,
  nodeRuntimeChanging,
  nodeRuntimeError,
  reloadNodeRuntime,
  setNodeRuntime,
  chooseNodeRuntime,
  logErrorBadgeEnabled,
  handleLogErrorBadgeToggle,
  modelHttpEnabled,
  modelHttpSaving,
  modelHttpEndpoint,
  publicModelConfigIds,
  enabledChatModels,
  allPublicModelsSelected,
  setModelHttpEnabled,
  saveModelHttpSettings,
  toggleAllPublicModels,
  copyModelHttpEndpoint,
} = useSettings();

const modelHttpDialogVisible = ref(false);
async function handleSaveModelHttpSettings() {
  try {
    await saveModelHttpSettings();
    modelHttpDialogVisible.value = false;
  } catch (error) {
    window.alert(`保存模型配置失败：${String(error)}`);
  }
}
</script>

<style scoped lang="scss">
@use "../styles/settings" as *;

.model-http-service-actions {
  display: grid;
  grid-template-columns: 70% 20%;
  column-gap: 10%;
  align-items: center;
}

.model-http-service-endpoint {
  display: flex;
  align-items: center;
  min-width: 0;
  height: 32px;
  padding-left: 12px;
  border: 1px solid var(--border);
  border-radius: var(--td-radius-default);
  background: var(--bg);

  code {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.t-button) {
    flex: none;
    height: 30px;
    margin-left: auto;
  }
}

.model-http-service-config-button {
  height: 32px;
}

.model-http-model-selection-header {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 16px;
}

.model-http-model-selection-list {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
  max-height: 360px;
  overflow-y: auto;
  padding: 4px 10px 4px 4px;
}

.model-http-model-option {
  min-width: 0;
  margin: 0;
  min-height: 60px;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: var(--td-radius-default);
  font-size: 16px;

  :deep(.t-checkbox__label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 18px;
    line-height: 26px;
  }

}
</style>
