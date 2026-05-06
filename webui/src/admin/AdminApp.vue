<template>
  <div class="admin-shell">
    <aside class="admin-sidebar">
      <div class="admin-brand">
        <div class="admin-brand-mark">
          <img class="admin-brand-mark-image" :src="brandLogoSrc" alt="Zihuan Next icon" />
        </div>
        <div>
          <h1>Zihuan Next</h1>
          <p>AI框架管理界面</p>
        </div>
      </div>
      <nav class="admin-nav">
        <RouterLink class="admin-nav-link" to="/">仪表盘</RouterLink>
        <RouterLink class="admin-nav-link" to="/connections">连接配置</RouterLink>
        <RouterLink class="admin-nav-link" to="/llm">模型配置</RouterLink>
        <RouterLink class="admin-nav-link" to="/agents">Agent 管理</RouterLink>
        <RouterLink class="admin-nav-link" to="/graphs">节点图与工作流</RouterLink>
        <RouterLink class="admin-nav-link" to="/tasks">任务管理器</RouterLink>
      </nav>
      <section class="admin-theme-card">
        <div class="admin-theme-card-header">
          <div>
            <strong>主题</strong>
          </div>
          <span class="theme-mode-pill">{{ currentThemeSchemaLabel }}</span>
        </div>

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
      </section>
    </aside>
    <main class="admin-main">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { RouterLink, RouterView } from "vue-router";
import brandLogoSrc from "../assets/brand-icon.png";

import {
  clearTheme,
  getCurrentThemeName,
  getStoredThemeName,
  getThemeConfig,
  getThemeNames,
  onThemeChange,
  setTheme,
} from "../ui/theme";

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

const currentThemeLabel = computed(() => currentTheme.value?.display_name ?? currentThemeName.value);

const currentThemeSchemaLabel = computed(() => currentTheme.value?.schema === "light" ? "亮色" : "暗色");

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
    background: config.css["--btn-primary"] ?? config.css["--accent"] ?? config.litegraph.nodeBox,
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
</script>
