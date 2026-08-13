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
import { logErrorBadgeEnabled, setLogErrorBadgeEnabled } from "../state/logStream";
import type { LlmConfig } from "../../api/client";

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

type PythonRuntimeKind = "uv_project" | "project_venv" | "custom_executable";

interface PythonRuntimeResponse {
  config: { kind: PythonRuntimeKind; executable_path?: string | null };
  available: boolean;
  command: string | null;
  executable_path: string | null;
  version: string | null;
  diagnostic: string | null;
}

interface PythonRuntimeSelectionResponse {
  cancelled: boolean;
  runtime: PythonRuntimeResponse | null;
}

interface ModelHttpApiKey {
  id: string;
  name: string;
  secret_prefix: string;
  created_at: string;
  expires_at: string | null;
  group: string | null;
  enabled: boolean;
}

interface ModelHttpSettingsResponse {
  enabled: boolean;
  public_model_config_ids: string[];
  api_keys: ModelHttpApiKey[];
}

interface PublicChatModel {
  config_id: string;
  name: string;
  model_name: string;
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

  function handleThemeChange(next: string): void {
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

  const pythonRuntime = ref<PythonRuntimeResponse | null>(null);
  const pythonRuntimeLoading = ref(false);
  const pythonRuntimeChanging = ref(false);
  const pythonRuntimeError = ref<string | null>(null);

  async function reloadPythonRuntime() {
    pythonRuntimeLoading.value = true;
    pythonRuntimeError.value = null;
    try {
      const response = await request<PythonRuntimeResponse>("GET", "/settings/python-runtime");
      pythonRuntime.value = response;
    } catch (error) {
      pythonRuntimeError.value = String(error);
    } finally {
      pythonRuntimeLoading.value = false;
    }
  }

  onMounted(reloadPythonRuntime);

  const modelHttpEnabled = ref(false);
  const modelHttpSaving = ref(false);
  const publicModelConfigIds = ref<string[]>([]);
  const modelHttpApiKeys = ref<ModelHttpApiKey[]>([]);
  const enabledChatModels = ref<PublicChatModel[]>([]);
  const newModelHttpSecret = ref("");

  const allPublicModelsSelected = computed(
    () => enabledChatModels.value.length > 0 && publicModelConfigIds.value.length === enabledChatModels.value.length,
  );

  async function loadModelHttpSettings() {
    const [settings, llmConfigs] = await Promise.all([
      request<ModelHttpSettingsResponse>("GET", "/settings/model-http"),
      request<LlmConfig[]>("GET", "/system/llm-refs"),
    ]);
    modelHttpEnabled.value = settings.enabled;
    publicModelConfigIds.value = settings.public_model_config_ids;
    modelHttpApiKeys.value = settings.api_keys;
    enabledChatModels.value = llmConfigs
      .filter(
        (item): item is LlmConfig & { model: Extract<LlmConfig["model"], { type: "chat_llm" }> } =>
          item.enabled && item.model.type === "chat_llm",
      )
      .map((item) => ({ config_id: item.config_id, name: item.name, model_name: item.model.llm.model_name }));
  }

  async function saveModelHttpSettings() {
    modelHttpSaving.value = true;
    try {
      const response = await request<ModelHttpSettingsResponse>("PUT", "/settings/model-http", {
        enabled: modelHttpEnabled.value,
        public_model_config_ids: publicModelConfigIds.value,
      });
      modelHttpEnabled.value = response.enabled;
      publicModelConfigIds.value = response.public_model_config_ids;
      modelHttpApiKeys.value = response.api_keys;
    } finally {
      modelHttpSaving.value = false;
    }
  }

  async function setModelHttpEnabled(enabled: boolean) {
    modelHttpEnabled.value = enabled;
    await saveModelHttpSettings();
  }

  function toggleAllPublicModels(checked: boolean) {
    publicModelConfigIds.value = checked ? enabledChatModels.value.map((item) => item.config_id) : [];
  }

  async function createModelHttpApiKey() {
    const name = window.prompt("API Key 名称");
    if (!name?.trim()) return;
    const expiresAt = window.prompt("过期时间（RFC3339，可留空表示永久）", "")?.trim() || null;
    const group = window.prompt("分组（可留空）", "")?.trim() || null;
    const response = await request<{ secret: string } & ModelHttpApiKey>("POST", "/settings/model-http/api-keys", {
      name: name.trim(), expires_at: expiresAt, group,
    });
    modelHttpApiKeys.value.push(response);
    newModelHttpSecret.value = response.secret;
  }

  async function updateModelHttpApiKey(key: ModelHttpApiKey, patch: Partial<ModelHttpApiKey>) {
    const response = await request<ModelHttpApiKey>("PUT", `/settings/model-http/api-keys/${key.id}`, {
      name: patch.name ?? key.name,
      expires_at: patch.expires_at ?? key.expires_at,
      group: patch.group ?? key.group,
      enabled: patch.enabled ?? key.enabled,
    });
    const index = modelHttpApiKeys.value.findIndex((item) => item.id === key.id);
    if (index >= 0) modelHttpApiKeys.value[index] = response;
  }

  async function deleteModelHttpApiKey(id: string) {
    await request("DELETE", `/settings/model-http/api-keys/${id}`);
    modelHttpApiKeys.value = modelHttpApiKeys.value.filter((item) => item.id !== id);
  }

  async function copyModelHttpSecret() {
    await navigator.clipboard.writeText(newModelHttpSecret.value);
  }

  onMounted(() => { void loadModelHttpSettings(); });

  async function changePythonRuntime() {
    pythonRuntimeChanging.value = true;
    pythonRuntimeError.value = null;
    try {
      const response = await request<PythonRuntimeSelectionResponse>(
        "POST",
        "/settings/python-runtime/select",
      );
      if (response.runtime) {
        pythonRuntime.value = response.runtime;
      }
    } catch (error) {
      pythonRuntimeError.value = String(error);
    } finally {
      pythonRuntimeChanging.value = false;
    }
  }

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

  function handleLogErrorBadgeToggle(checked: boolean) {
    setLogErrorBadgeEnabled(checked);
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
    pythonRuntime,
    pythonRuntimeLoading,
    pythonRuntimeChanging,
    pythonRuntimeError,
    reloadPythonRuntime,
    changePythonRuntime,
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
    restoreFileInput,
    restoreLoading,
    restoreError,
    restoreSuccess,
    triggerRestorePicker,
    handleRestoreFileChange,
    handleExportConfig,
    logErrorBadgeEnabled,
    handleLogErrorBadgeToggle,
  };
}

export type UseSettingsReturn = ReturnType<typeof useSettings>;
