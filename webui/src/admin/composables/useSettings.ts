import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import { request } from "../../api/client";
import {
  clearTheme,
  getCurrentThemeName,
  getStoredThemeName,
  getThemeConfig,
  getThemeNames,
  onThemeChange,
  setTheme,
} from "../../ui/theme";

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


export function useSettings() {
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

  const restoreFileInput = ref<HTMLInputElement | null>(null);
  const restoreLoading = ref(false);
  const restoreError = ref<string | null>(null);
  const restoreSuccess = ref(false);

  function triggerRestorePicker() {
    restoreError.value = null;
    restoreSuccess.value = false;
    restoreFileInput.value?.click();
  }

  async function handleRestoreFileChange(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    restoreLoading.value = true;
    restoreError.value = null;
    restoreSuccess.value = false;

    try {
      const arrayBuffer = await file.arrayBuffer();
      const response = await fetch("/api/settings/config-restore", {
        method: "POST",
        headers: { "Content-Type": "application/zip" },
        body: arrayBuffer,
      });
      const json = await response.json();
      if (!response.ok) {
        restoreError.value = json?.error ?? `HTTP ${response.status}`;
      } else {
        restoreSuccess.value = true;
      }
    } catch (e) {
      restoreError.value = String(e);
    } finally {
      restoreLoading.value = false;
      input.value = "";
    }
  }

  function handleExportConfig() {
    const a = document.createElement("a");
    a.href = "/api/settings/config-export";
    a.download = "";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  }

  return {
    themeOptions,
    currentThemeName,
    selectedTheme,
    currentTheme,
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
  };
}

export type UseSettingsReturn = ReturnType<typeof useSettings>;
