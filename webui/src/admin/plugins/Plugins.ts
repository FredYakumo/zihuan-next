import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute } from "vue-router";

import { pluginsApi, setup as setupApi, type DetailedSetupConfig, type EnvironmentInfo, type PluginRecord, type SetupProgressEvent } from "../../api/client";
import elasticsearchIcon from "../../assets/icons/elasticsearch.svg";
import mysqlIcon from "../../assets/icons/mysql.svg";
import redisIcon from "../../assets/icons/redis.svg";
import rustfsIcon from "../../assets/icons/rustfs.svg";
import sqliteIcon from "../../assets/icons/sqlite.svg";
import weaviateIcon from "../../assets/icons/weaviate.svg";

type InstallStep = "picker" | "config" | "progress";
type PluginInstallMethod = "docker" | "binary" | "command_docker" | "command_binary";

export function usePluginsPage() {
  const route = useRoute();

  const plugins = ref<PluginRecord[]>([]); const loading = ref(false); const saving = ref(false); const installVisible = ref(false); const installStep = ref<InstallStep>("picker");
  const successVisible = ref(false); const successInstallCommand = ref<string | null>(null); const successConnectionConfig = ref<unknown>(undefined);
  const uninstallCommandVisible = ref(false); const uninstallCommand = ref("");
  const exitConfirmVisible = ref(false); const error = ref(""); const progress = ref<SetupProgressEvent[]>([]); const taskId = ref("");
  let cleanupProgress = (() => {}) as () => void;
  let completionTimer: ReturnType<typeof setTimeout> | null = null;
  let installAttempt = 0;
  const componentOptions = [
    { value: "mysql", label: "MySQL", desc: "关系数据库", icon: mysqlIcon },
    { value: "sqlite", label: "SQLite3", desc: "关系数据库", icon: sqliteIcon },
    { value: "redis", label: "Redis", desc: "缓存", icon: redisIcon },
    { value: "weaviate", label: "Weaviate", desc: "检索数据库", icon: weaviateIcon },
    { value: "elasticsearch", label: "Elasticsearch", desc: "检索数据库", icon: elasticsearchIcon },
    { value: "rustfs", label: "RustFS", desc: "对象存储", icon: rustfsIcon },
  ] as const;
  const form = reactive({ name: "", version: "latest", component_type: "mysql", install_method: "docker" as PluginInstallMethod });
  const selectedComponent = computed(() => componentOptions.find((item) => item.value === form.component_type) ?? componentOptions[0]);
  const environmentLoading = ref(false);
  const environment = ref<EnvironmentInfo>({ os: "", os_detail: "", docker_available: false, docker_compose_available: false, binary_install_available: false, binary_install_reason: null, wsl_available: null, wsl_docker_available: null, cuda_version: null, compiler_version: null, proxy: null, services: [] });
  const dockerSupported = computed(() => environment.value.docker_compose_available);
  const binarySupported = computed(() => environment.value.binary_install_available);
  const dockerUnsupportedReason = "Docker Compose 不可用，请安装并启动 Docker Desktop 或 Docker Compose";
  const binaryUnsupportedReason = computed(() => environment.value.binary_install_reason ?? "当前系统不支持本机 native 程序安装");
  const config = reactive<DetailedSetupConfig>({ install_method: "docker", target_machine_address: "", expose_public_access: false, use_target_machine_address: false,
    relational: { enabled: true, source: "install", type: "mysql", deployment: { image: "mysql:8.4", port: 3306, data_dir: "./data/plugin-mysql", container_name: "zihuan-plugin-mysql", restart_policy: "unless-stopped" }, host: "127.0.0.1", username: "root", password: "", database: "zihuan", sqlite_path: "", max_connections: 32, acquire_timeout_secs: 30 },
    rustfs: { enabled: false, source: "install", deployment: { image: "rustfs/rustfs:latest", port: 9000, data_dir: "./data/plugin-rustfs", container_name: "zihuan-plugin-rustfs", restart_policy: "unless-stopped" }, endpoint: "http://127.0.0.1:9000", bucket: "zihuan", region: "us-east-1", access_key: "", secret_key: "", public_base_url: null, path_style: true },
    search: { enabled: false, source: "install", type: "weaviate", deployment: { image: "cr.weaviate.io/semitechnologies/weaviate:1.30.5", port: 8080, data_dir: "./data/plugin-search", container_name: "zihuan-plugin-search", restart_policy: "unless-stopped" }, base_url: "http://127.0.0.1:8080", username: null, password: null, api_key: null, auth_method: "api_key", vector_dimensions: 1024 },
    redis: { enabled: false, source: "install", deployment: { image: "redis:7", port: 6379, data_dir: "./data/plugin-redis", container_name: "zihuan-plugin-redis", restart_policy: "unless-stopped" }, url: "redis://127.0.0.1:6379", username: null, password: null } });
  const columns = [{ colKey: "name", title: "插件名称" }, { colKey: "component_type", title: "组件", width: 130 }, { colKey: "installation_method", title: "安装方式", width: 150 }, { colKey: "status", title: "状态", width: 110 }, { colKey: "connection_ids", title: "关联连接", width: 100 }, { colKey: "actions", title: "操作", width: 370 }];
  function statusLabel(value: string) { return ({ installing: "安装中", installed: "已启用", disabled: "已停用", failed: "失败", command_generated: "命令已生成" } as Record<string, string>)[value] ?? value; }
  function statusTheme(value: string) { return value === "installed" ? "success" : value === "failed" ? "danger" : value === "disabled" ? "warning" : "primary"; }
  async function load() { loading.value = true; try { plugins.value = await pluginsApi.list(); } catch (e) { error.value = String(e); } finally { loading.value = false; } }
  function resetConfig() { config.relational.enabled = form.component_type === "mysql" || form.component_type === "sqlite"; config.redis.enabled = form.component_type === "redis"; config.search.enabled = form.component_type === "weaviate" || form.component_type === "elasticsearch"; config.rustfs.enabled = form.component_type === "rustfs"; config.search.type = form.component_type === "elasticsearch" ? "elasticsearch" : "weaviate"; config.install_method = form.install_method.endsWith("docker") ? "docker" : "binary"; }
  function applyComponentDefaults() {
    const component = form.component_type;
    if (component === "mysql") Object.assign(config.relational, { type: "mysql", deployment: { image: "mysql:8.4", port: 3306, data_dir: "./data/plugin-mysql", container_name: "zihuan-plugin-mysql", restart_policy: "unless-stopped" }, host: "127.0.0.1", username: "root", password: "", database: "zihuan" });
    if (component === "sqlite") Object.assign(config.relational, { type: "sqlite", sqlite_path: "" });
    if (component === "redis") Object.assign(config.redis, { deployment: { image: "redis:7", port: 6379, data_dir: "./data/plugin-redis", container_name: "zihuan-plugin-redis", restart_policy: "unless-stopped" }, url: "redis://127.0.0.1:6379", username: null, password: null });
    if (component === "rustfs") Object.assign(config.rustfs, { deployment: { image: "rustfs/rustfs:latest", port: 9000, data_dir: "./data/plugin-rustfs", container_name: "zihuan-plugin-rustfs", restart_policy: "unless-stopped" }, endpoint: "http://127.0.0.1:9000", bucket: "zihuan", access_key: "", secret_key: "" });
    if (component === "weaviate") Object.assign(config.search, { type: "weaviate", deployment: { image: "cr.weaviate.io/semitechnologies/weaviate:1.30.5", port: 8080, data_dir: "./data/plugin-weaviate", container_name: "zihuan-plugin-weaviate", restart_policy: "unless-stopped" }, base_url: "http://127.0.0.1:8080", username: null, password: null, api_key: "", auth_method: "api_key" });
    if (component === "elasticsearch") Object.assign(config.search, { type: "elasticsearch", deployment: { image: "docker.elastic.co/elasticsearch/elasticsearch:8.15.0", port: 9200, data_dir: "./data/plugin-elasticsearch", container_name: "zihuan-plugin-elasticsearch", restart_policy: "unless-stopped" }, base_url: "http://127.0.0.1:9200", username: "elastic", password: "", api_key: null, auth_method: "password" });
    form.install_method = getPreferredInstallMethod();
    resetConfig();
  }
  watch(() => form.component_type, applyComponentDefaults);
  function getPreferredInstallMethod(): PluginInstallMethod {
    if (environment.value.docker_compose_available) return "docker";
    if (environment.value.binary_install_available) return "binary";
    return "command_docker";
  }
  async function detectEnvironment() {
    environmentLoading.value = true;
    try {
      environment.value = await setupApi.getEnvironment();
    } catch (e) {
      console.error("Failed to detect plugin install environment", e);
    } finally {
      form.install_method = getPreferredInstallMethod();
      environmentLoading.value = false;
    }
  }
  function randomPluginName() { return `${selectedComponent.value.value}-plugin-${Math.floor(1000 + Math.random() * 9000)}`; }
  function selectComponent(value: string) { form.component_type = value; form.name = randomPluginName(); installStep.value = "config"; }
  function clearInstallationSideEffects() {
    cleanupProgress();
    cleanupProgress = () => {};
    if (completionTimer !== null) {
      clearTimeout(completionTimer);
      completionTimer = null;
    }
  }
  function resetInstallationState() {
    clearInstallationSideEffects();
    saving.value = false;
    error.value = "";
    progress.value = [];
    taskId.value = "";
    installStep.value = "picker";
  }
  function openInstall() {
    installAttempt += 1;
    resetInstallationState();
    installVisible.value = true;
    successVisible.value = false;
    applyComponentDefaults();
    void detectEnvironment();
  }
  function closeInstall() {
    installAttempt += 1;
    resetInstallationState();
    exitConfirmVisible.value = false;
    installVisible.value = false;
    void load();
  }
  function requestCloseInstall() {
    if (saving.value) {
      exitConfirmVisible.value = true;
      return;
    }
    closeInstall();
  }
  function backFromConfig() {
    if (saving.value) {
      exitConfirmVisible.value = true;
      return;
    }
    installStep.value = "picker";
  }
  function confirmExitInstallation() { closeInstall(); }
  function backFromProgress() {
    if (saving.value) {
      exitConfirmVisible.value = true;
      return;
    }
    clearInstallationSideEffects();
    error.value = "";
    progress.value = [];
    taskId.value = "";
    installStep.value = "config";
  }
  async function showInstallationSuccess(attempt: number, installCommand?: string, connectionConfig?: unknown) {
    await load();
    if (attempt !== installAttempt) return;
    clearInstallationSideEffects();
    installVisible.value = false;
    successInstallCommand.value = installCommand ?? null;
    successConnectionConfig.value = connectionConfig;
    successVisible.value = true;
  }
  function confirmInstallationSuccess() {
    successVisible.value = false;
    successInstallCommand.value = null;
    successConnectionConfig.value = undefined;
    resetInstallationState();
  }
  async function install() {
    if (!form.name.trim()) { error.value = "请填写插件名称"; return; }
    // The retry button is only offered from the progress step. The background
    // task may have finished regardless of the stream state, so check the
    // record before submitting a second installation for the same name.
    if (installStep.value === "progress") {
      try {
        const list = await pluginsApi.list();
        const existing = list.find((item) => item.name === form.name.trim());
        if (existing?.status === "installed") {
          plugins.value = list;
          await showInstallationSuccess(installAttempt, metadataString(existing, "install_command") ?? undefined, existing.extra_install_metadata?.connection_config);
          return;
        }
      } catch {
        // Fall through and let the install request report the real problem.
      }
    }
    const showProgress = form.component_type !== "sqlite" && (form.install_method === "docker" || form.install_method === "binary");
    const attempt = ++installAttempt;
    clearInstallationSideEffects();
    if (showProgress) installStep.value = "progress";
    progress.value = [];
    taskId.value = "";
    saving.value = true;
    error.value = "";
    resetConfig();
    let waitingForTask = false;
    try {
      const { install_method, ...plugin } = form;
      const result = await pluginsApi.install({ ...plugin, ...(form.component_type === "sqlite" ? {} : { install_method, detailed_config: config }) });
      if (attempt !== installAttempt) {
        void load();
        return;
      }
      if (result.task_id) {
        waitingForTask = true;
        taskId.value = result.task_id;
        cleanupProgress = setupApi.streamProgress(result.task_id, (event) => {
          if (attempt !== installAttempt) return;
          progress.value.push(event);
          if (event.status === "error") {
            error.value = event.error ?? event.message;
            saving.value = false;
            clearInstallationSideEffects();
            return;
          }
          if (event.step === "finished") {
            clearInstallationSideEffects();
            saving.value = false;
            completionTimer = setTimeout(() => {
              completionTimer = null;
              void showInstallationSuccess(attempt, result.install_command, result.connections);
            }, 500);
          }
        }, async () => {
          if (attempt !== installAttempt || !saving.value) return;
          clearInstallationSideEffects();
          saving.value = false;
          // The task may have finished right before the stream closed; read the
          // record back so the dialog reports the real outcome instead of a
          // misleading disconnection warning.
          try {
            const list = await pluginsApi.list();
            if (attempt !== installAttempt) return;
            plugins.value = list;
            const record = list.find((item) => item.id === result.plugin?.id);
            if (record?.status === "installed") {
              await showInstallationSuccess(attempt, result.install_command, result.connections);
              return;
            }
            if (record?.status === "failed") {
              const lastError = record.extra_install_metadata?.last_error;
              error.value = typeof lastError === "string" && lastError.trim() ? lastError : "安装失败。";
              return;
            }
          } catch {
            // Fall through to the generic disconnection message.
          }
          error.value = "安装进度连接已断开。后台任务可能仍在运行，请返回插件列表确认状态后再重试。";
        });
        return;
      }
      saving.value = false;
      await showInstallationSuccess(attempt, result.install_command, result.connections);
    } catch (e) {
      if (attempt !== installAttempt) return;
      error.value = String(e);
    } finally {
      if (attempt === installAttempt && !waitingForTask) saving.value = false;
    }
  }
  async function toggle(plugin: PluginRecord) { try { const result = plugin.status === "installed" ? await pluginsApi.disable(plugin.id) : await pluginsApi.enable(plugin.id); if (result.command) { await navigator.clipboard.writeText(result.command); window.alert(`命令已复制：\n${result.command}`); } await load(); } catch (e) { error.value = String(e); } }
  function metadataString(plugin: PluginRecord, key: string): string | null { const value = plugin.extra_install_metadata?.[key]; return typeof value === "string" && value.trim() ? value : null; }
  function statusTitle(plugin: PluginRecord): string | undefined { return plugin.status === "failed" ? metadataString(plugin, "last_error") ?? undefined : undefined; }
  function installCommand(plugin: PluginRecord): string | null { return metadataString(plugin, "install_command"); }
  function connectionConfigJson(plugin: PluginRecord): string | null { const value = plugin.extra_install_metadata?.connection_config; return value == null ? null : JSON.stringify(value, null, 2); }
  async function copyPluginValue(value: string, label: string) { try { await navigator.clipboard.writeText(value); } catch (e) { error.value = `复制${label}失败：${String(e)}`; } }
  async function removePlugin(id: string) { try { const result = await pluginsApi.remove(id); await load(); if (result.uninstall_command) { uninstallCommand.value = result.uninstall_command; uninstallCommandVisible.value = true; } } catch (e) { error.value = String(e); } }
  onMounted(() => { void load(); if (route.query.install === "1") openInstall(); });
  onBeforeUnmount(() => {
    installAttempt += 1;
    clearInstallationSideEffects();
  });

  return {
    plugins,
    loading,
    saving,
    installVisible,
    installStep,
    successVisible,
    successInstallCommand,
    successConnectionConfig,
    uninstallCommandVisible,
    uninstallCommand,
    exitConfirmVisible,
    error,
    progress,
    taskId,
    componentOptions,
    form,
    selectedComponent,
    environmentLoading,
    dockerSupported,
    binarySupported,
    dockerUnsupportedReason,
    binaryUnsupportedReason,
    config,
    columns,
    statusLabel,
    statusTheme,
    statusTitle,
    load,
    selectComponent,
    openInstall,
    requestCloseInstall,
    backFromConfig,
    confirmExitInstallation,
    backFromProgress,
    confirmInstallationSuccess,
    install,
    toggle,
    installCommand,
    connectionConfigJson,
    copyPluginValue,
    removePlugin,
  };
}
