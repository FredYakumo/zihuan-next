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
    </div>

    <t-card title="Python 运行时" bordered header-bordered>
      <template #actions>
        <t-button variant="text" :disabled="pythonRuntimeLoading" @click="reloadPythonRuntime">
          <t-loading v-if="pythonRuntimeLoading" size="small" />
          {{ pythonRuntimeLoading ? "检测中…" : "重新检查" }}
        </t-button>
      </template>
      <p class="muted">Python 工具默认使用的解释器，可由单个工具覆盖。</p>

      <div class="settings-python-body">
        <div v-if="pythonRuntime" class="settings-python-status">
          <t-tag variant="light" :theme="pythonRuntime.available ? 'success' : 'warning'">
            {{ pythonRuntime.available ? "可用" : "不可用" }}
          </t-tag>
          <span v-if="pythonRuntime.executable_path" class="settings-path-label">Python 路径</span>
          <code v-if="pythonRuntime.executable_path" class="settings-path-value">{{ pythonRuntime.executable_path }}</code>
          <span v-if="pythonRuntime.version" class="muted">{{ pythonRuntime.version }}</span>
          <span v-if="pythonRuntime.diagnostic" class="settings-python-error">{{ pythonRuntime.diagnostic }}</span>
        </div>
        <div v-else class="settings-python-pending">
          <t-loading size="small" />
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
} = useSettings();
</script>

<style scoped lang="scss">
@use "../styles/settings" as *;
</style>
