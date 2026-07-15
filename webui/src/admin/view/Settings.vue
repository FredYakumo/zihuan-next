<template>
  <section class="page settings-page">
    <div class="page-hero">
      <h2>设置</h2>
    </div>

    <!-- Theme -->
    <section class="panel settings-section">
      <div class="split-header">
        <div>
          <h3>主题</h3>
          <p class="muted">调整界面配色方案。</p>
        </div>
        <span class="theme-mode-pill">{{ currentThemeSchemaLabel }}</span>
      </div>

      <div class="settings-theme-body">
        <label class="theme-select-field">
          <span>当前方案</span>
          <select :value="selectedTheme" @change="handleThemeChange">
            <option value="system">跟随系统</option>
            <option v-for="theme in themeOptions" :key="theme.name" :value="theme.name">
              {{ theme.display_name }}
            </option>
          </select>
        </label>

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
    </section>

    <section class="panel settings-section">
      <div class="split-header">
        <div>
          <h3>Python 运行时</h3>
          <p class="muted">Python 工具默认使用的解释器，可由单个工具覆盖。</p>
        </div>
        <button class="btn ghost" :disabled="pythonRuntimeLoading" @click="reloadPythonRuntime">
          {{ pythonRuntimeLoading ? "检测中…" : "刷新" }}
        </button>
      </div>

      <div class="settings-python-body">
        <div v-if="pythonRuntime" class="settings-python-status">
          <span class="badge" :class="pythonRuntime.available ? 'badge-ok' : 'badge-missing'">
            {{ pythonRuntime.available ? "可用" : "不可用" }}
          </span>
          <span v-if="pythonRuntime.executable_path" class="settings-path-label">Python 路径</span>
          <code v-if="pythonRuntime.executable_path" class="settings-path-value">{{ pythonRuntime.executable_path }}</code>
          <span v-if="pythonRuntime.version" class="muted">{{ pythonRuntime.version }}</span>
          <span v-if="pythonRuntime.diagnostic" class="settings-python-error">{{ pythonRuntime.diagnostic }}</span>
        </div>
        <div v-else-if="pythonRuntimeLoading" class="empty-state">加载中…</div>

        <div class="settings-backup-actions">
          <button class="btn primary" :disabled="pythonRuntimeChanging" @click="changePythonRuntime">
            {{ pythonRuntimeChanging ? "选择中…" : "更改" }}
          </button>
          <span v-if="pythonRuntimeError" class="settings-python-error">{{ pythonRuntimeError }}</span>
        </div>
      </div>
    </section>

    <!-- Data directory -->
    <section class="panel settings-section">
      <div class="split-header">
        <div>
          <h3>数据目录</h3>
          <p class="muted">应用运行时产生的数据的存储位置。</p>
        </div>
        <button class="btn ghost" @click="reloadStorageInfo">刷新</button>
      </div>

      <div v-if="storageInfo" class="settings-storage-body">
        <div class="settings-path-row settings-path-row--root">
          <span class="settings-path-label">应用数据目录</span>
          <code class="settings-path-value">{{ storageInfo.data_dir }}</code>
        </div>
        <div
          v-for="entry in storageInfo.storage_entries"
          :key="entry.path"
          class="settings-path-row"
        >
          <span class="settings-path-label">{{ entry.label }}</span>
          <code class="settings-path-value">{{ entry.path }}</code>
          <span class="badge" :class="entry.exists ? 'badge-ok' : 'badge-missing'">
            {{ entry.exists ? "存在" : "未创建" }}
          </span>
        </div>
      </div>
      <div v-else-if="storageLoading" class="empty-state">加载中…</div>
      <div v-else-if="storageError" class="empty-state">{{ storageError }}</div>
    </section>

    <!-- Config backup / restore -->
    <section class="panel settings-section">
      <div class="split-header">
        <div>
          <h3>配置备份</h3>
        </div>
        <div class="settings-backup-actions">
          <button class="btn ghost" @click="handleExportConfig">导出配置</button>
          <button class="btn primary" :disabled="restoreLoading" @click="triggerRestorePicker">
            {{ restoreLoading ? "恢复中…" : "恢复配置" }}
          </button>
          <input
            ref="restoreFileInput"
            type="file"
            accept=".zip"
            class="settings-backup-file-input"
            @change="handleRestoreFileChange"
          />
        </div>
      </div>

      <div v-if="restoreSuccess" class="settings-backup-feedback settings-backup-feedback--ok">
        配置已成功恢复，请重启服务以使新配置生效。
      </div>
      <div v-if="restoreError" class="settings-backup-feedback settings-backup-feedback--err">
        {{ restoreError }}
      </div>
    </section>

    <!-- Local model files -->
    <section
      v-for="group in modelGroups"
      :key="group.label"
      class="panel settings-section"
    >
      <div class="split-header">
        <div>
          <h3>本地模型 — {{ group.label }}</h3>
          <p class="muted">{{ group.dir }}</p>
        </div>
      </div>

      <div v-if="group.models.length === 0" class="empty-state">
        该目录下暂无可用模型。
      </div>
      <div v-else class="settings-model-list">
        <article
          v-for="model in group.models"
          :key="model.name"
          class="settings-model-card"
        >
          <div class="settings-model-header">
            <strong>{{ model.name }}</strong>
            <span class="badge" :class="model.valid ? 'badge-ok' : 'badge-warn'">
              {{ model.valid ? "就绪" : "不完整" }}
            </span>
          </div>
          <code class="settings-path-value">{{ model.path }}</code>
          <div v-if="model.size_bytes != null" class="settings-model-meta">
            <span class="muted">大小：{{ formatBytes(model.size_bytes) }}</span>
          </div>
        </article>
      </div>
    </section>
  </section>
</template>

<script setup lang="ts">
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
} = useSettings();
</script>

<style scoped lang="scss">
@use "../styles/settings" as *;
</style>
