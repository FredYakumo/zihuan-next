import { computed, onMounted, ref } from "vue";
import { pluginsApi, type PluginRecord, type ScriptPluginDetail, type ScriptPluginFile, type ScriptPluginFileNode } from "../../api/client";

export function usePluginsPage() {
  const plugins = ref<PluginRecord[]>([]);
  const loading = ref(false);
  const error = ref("");
  const detailVisible = ref(false);
  const detailLoading = ref(false);
  const detail = ref<ScriptPluginDetail | null>(null);
  const detailName = ref("");
  const detailError = ref("");
  const sourceOpen = ref(false);
  const activeFile = ref("");
  const fileContent = ref<ScriptPluginFile | null>(null);
  const fileLoading = ref(false);
  const fileError = ref("");
  const zipVisible = ref(false);
  const zipFile = ref<File | null>(null);
  const zipInstalling = ref(false);
  const zipError = ref("");
  const zipWarnings = ref<string[]>([]);
  const flatten = (nodes: ScriptPluginFileNode[]): ScriptPluginFileNode[] => nodes.flatMap((node) => node.kind === "file" ? [node] : flatten(node.children));
  const detailFileList = computed(() => detail.value ? flatten(detail.value.files) : []);
  const columns = [
    { colKey: "name", title: "插件名称" },
    { colKey: "version", title: "版本", width: 120 },
    { colKey: "plugin_type", title: "类型", width: 180 },
    { colKey: "actions", title: "操作", width: 180 },
  ];
  async function load() {
    loading.value = true;
    try { plugins.value = await pluginsApi.list(); }
    catch (failure) { error.value = String(failure); }
    finally { loading.value = false; }
  }
  async function openDetail(plugin: PluginRecord) {
    detailName.value = plugin.name;
    detailVisible.value = true;
    detailLoading.value = true;
    detailError.value = "";
    detail.value = null;
    sourceOpen.value = false;
    activeFile.value = "";
    fileContent.value = null;
    try {
      const loaded = await pluginsApi.scriptDetail(plugin.name);
      detail.value = loaded;
      const first = flatten(loaded.files).find((node) => node.name === "plugin.py")
        ?? flatten(loaded.files).find((node) => node.name === "plugin.mjs")
        ?? flatten(loaded.files)[0];
      if (first) void previewFile(first.path);
    } catch (e) {
      detailError.value = String(e);
    } finally {
      detailLoading.value = false;
    }
  }
  async function previewFile(path: string) {
    activeFile.value = path;
    fileLoading.value = true;
    fileError.value = "";
    fileContent.value = null;
    try {
      fileContent.value = await pluginsApi.scriptFile(detailName.value, path);
    } catch (e) {
      fileError.value = String(e);
    } finally {
      fileLoading.value = false;
    }
  }
  async function removeScriptPlugin(plugin: PluginRecord) {
    try {
      await pluginsApi.removeScript(plugin.name);
      if (detailVisible.value && detailName.value === plugin.name) detailVisible.value = false;
      await load();
    } catch (e) {
      error.value = String(e);
    }
  }
  function openZipInstall() {
    zipFile.value = null;
    zipError.value = "";
    zipWarnings.value = [];
    zipVisible.value = true;
  }
  function selectZipFile(file: File) { zipFile.value = file; zipError.value = ""; }
  async function installZip() {
    if (!zipFile.value) { zipError.value = "请选择插件 zip 包"; return; }
    zipInstalling.value = true;
    zipError.value = "";
    zipWarnings.value = [];
    try {
      const result = await pluginsApi.installZip(zipFile.value);
      zipWarnings.value = result.warnings ?? [];
      await load();
      if (!result.warnings?.length) zipVisible.value = false;
    } catch (e) {
      zipError.value = String(e);
    } finally {
      zipInstalling.value = false;
    }
  }

  onMounted(load);
  return {
    plugins, loading, error, columns, detailVisible, detailLoading, detail,
    detailName, detailError, sourceOpen, activeFile, fileContent, fileLoading,
    fileError, detailFileList, openDetail, previewFile, removeScriptPlugin,
    zipVisible, zipFile, zipInstalling, zipError, zipWarnings, openZipInstall,
    selectZipFile, installZip,
  };
}
