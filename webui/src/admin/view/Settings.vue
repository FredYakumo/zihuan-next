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
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  clearTheme,
  getCurrentThemeName,
  getStoredThemeName,
  getThemeConfig,
  getThemeNames,
  onThemeChange,
  setTheme,
} from "../../ui/theme";
import { request } from "../../api/client";

// ─── Theme ────────────────────────────────────────────────────────────────────

const themeOptions = getThemeNames();
const currentThemeName = ref(getCurrentThemeName());
const selectedTheme = ref(getStoredThemeName() ?? "system");

const stopListening = onThemeChange(() => {
  currentThemeName.value = getCurrentThemeName();
  selectedTheme.value = getStoredThemeName() ?? "system";
});

onBeforeUnmount(() => {
  stopListening();
});

const currentTheme = computed(() => getThemeConfig(currentThemeName.value));
const currentThemeLabel = computed(
  () => currentTheme.value?.display_name ?? currentThemeName.value
);
const currentThemeSchemaLabel = computed(() =>
  currentTheme.value?.schema === "light" ? "亮色" : "暗色"
);

const themePreviewStyle = computed(() => {
  const config = currentTheme.value;
  if (!config) return {};
  return {
    background: config.css["--bg"] ?? config.litegraph.canvasBg,
    color: config.css["--text"] ?? config.litegraph.nodeTitleText,
    borderColor: config.css["--border"] ?? config.litegraph.widgetOutline,
  };
});

const themePreviewToolbarStyle = computed(() => {
  const config = currentTheme.value;
  if (!config) return {};
  return {
    background: config.css["--toolbar-bg"] ?? config.litegraph.nodeHeader,
    color: config.css["--text"] ?? config.litegraph.nodeTitleText,
  };
});

const themeAccentStyle = computed(() => {
  const config = currentTheme.value;
  if (!config) return {};
  return {
    background:
      config.css["--btn-primary"] ??
      config.css["--accent"] ??
      config.litegraph.nodeBox,
    color: config.css["--btn-primary-text"] ?? "#ffffff",
  };
});

function handleThemeChange(event: Event): void {
  const target = event.target as HTMLSelectElement;
  const next = target.value;
  if (next === "system") {
    clearTheme();
    selectedTheme.value = "system";
    return;
  }
  setTheme(next);
  selectedTheme.value = next;
}

// ─── Storage info ─────────────────────────────────────────────────────────────

interface StorageEntry {
  label: string;
  path: string;
  exists: boolean;
}

interface ModelEntry {
  name: string;
  path: string;
  valid: boolean;
  size_bytes: number | null;
}

interface ModelGroup {
  label: string;
  dir: string;
  models: ModelEntry[];
}

interface StorageInfoResponse {
  data_dir: string;
  storage_entries: StorageEntry[];
  model_groups: ModelGroup[];
}

const storageInfo = ref<StorageInfoResponse | null>(null);
const storageLoading = ref(false);
const storageError = ref<string | null>(null);
const modelGroups = ref<ModelGroup[]>([]);

async function reloadStorageInfo() {
  storageLoading.value = true;
  storageError.value = null;
  try {
    const data = await request<StorageInfoResponse>(
      "GET",
      "/settings/storage-info"
    );
    storageInfo.value = data;
    modelGroups.value = data.model_groups;
  } catch (e) {
    storageError.value = String(e);
  } finally {
    storageLoading.value = false;
  }
}

onMounted(reloadStorageInfo);

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
</script>

<style scoped lang="scss">
.settings-section {
  padding: 24px;
  display: grid;
  gap: 20px;
}

.settings-theme-body {
  display: grid;
  gap: 14px;
  max-width: 380px;
}

.settings-storage-body {
  display: grid;
  gap: 10px;
}

.settings-path-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 14px;
  background: color-mix(in srgb, var(--admin-bg-soft) 80%, transparent 20%);
  border: 1px solid var(--admin-border);
  flex-wrap: wrap;
}

.settings-path-row--root {
  background: color-mix(in srgb, var(--admin-accent-soft) 18%, var(--admin-bg-soft) 82%);
  border-color: color-mix(in srgb, var(--admin-accent) 24%, var(--admin-border) 76%);
}

.settings-path-label {
  font-size: 13px;
  font-weight: 700;
  color: var(--admin-subtle);
  white-space: nowrap;
  min-width: 100px;
}

.settings-path-value {
  flex: 1;
  font-size: 12px;
  color: var(--admin-ink);
  word-break: break-all;
  font-family: "Cascadia Code", "Fira Code", "Consolas", monospace;
  opacity: 0.85;
}

.badge-ok {
  background: color-mix(in srgb, var(--admin-good-bg) 80%, transparent 20%);
  color: var(--admin-good);
  border-color: color-mix(in srgb, var(--admin-good) 30%, transparent 70%);
}

.badge-missing {
  background: color-mix(in srgb, var(--admin-warn-bg) 80%, transparent 20%);
  color: var(--admin-warn);
  border-color: color-mix(in srgb, var(--admin-warn) 30%, transparent 70%);
}

.badge-warn {
  background: color-mix(in srgb, var(--admin-warn-bg) 80%, transparent 20%);
  color: var(--admin-warn);
  border-color: color-mix(in srgb, var(--admin-warn) 30%, transparent 70%);
}

.settings-model-list {
  display: grid;
  gap: 12px;
}

.settings-model-card {
  padding: 16px;
  border-radius: 18px;
  border: 1px solid var(--admin-border);
  background: color-mix(in srgb, var(--admin-bg-soft) 72%, transparent 28%);
  display: grid;
  gap: 8px;
}

.settings-model-header {
  display: flex;
  align-items: center;
  gap: 10px;
}

.settings-model-header strong {
  font-size: 14px;
}

.settings-model-meta {
  font-size: 12px;
}
</style>
