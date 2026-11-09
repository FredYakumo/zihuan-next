import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute } from "vue-router";

import { pluginsApi, setup as setupApi, type DetailedSetupConfig, type EnvironmentInfo, type PluginRecord, type ScriptPluginDetail, type ScriptPluginFile, type ScriptPluginFileNode, type SetupProgressEvent } from "../../api/client";
import type { WechatLoginStatusResult } from "../../api/client";
import elasticsearchIcon from "../../assets/icons/elasticsearch.svg";
import mysqlIcon from "../../assets/icons/mysql.svg";
import redisIcon from "../../assets/icons/redis.svg";
import rustfsIcon from "../../assets/icons/rustfs.svg";
import sqliteIcon from "../../assets/icons/sqlite.svg";
import weaviateIcon from "../../assets/icons/weaviate.svg";
import wechatIcon from "../../assets/icons/wechat.svg";

type InstallStep = "picker" | "config" | "progress";
type PluginInstallMethod = "docker" | "binary" | "command_docker" | "command_binary";

export function usePluginsPage() {
  const route = useRoute();

  const plugins = ref<PluginRecord[]>([]); const loading = ref(false); const saving = ref(false); const installVisible = ref(false); const installStep = ref<InstallStep>("picker");
  const successVisible = ref(false); const successInstallCommand = ref<string | null>(null); const successConnectionConfig = ref<unknown>(undefined);
  const uninstallCommandVisible = ref(false); const uninstallCommand = ref("");
  const exitConfirmVisible = ref(false); const error = ref(""); const progress = ref<SetupProgressEvent[]>([]); const taskId = ref("");
  // Detail dialog: manifest, dependency list and a collapsible source viewer.
  const detailVisible = ref(false); const detailLoading = ref(false); const detail = ref<ScriptPluginDetail | null>(null); const detailName = ref("");
  const detailError = ref(""); const sourceOpen = ref(false);
  const flatten = (nodes: ScriptPluginFileNode[]): ScriptPluginFileNode[] => nodes.flatMap((node) => node.kind === "file" ? [node] : flatten(node.children));
  const activeFile = ref(""); const fileContent = ref<ScriptPluginFile | null>(null); const fileLoading = ref(false); const fileError = ref("");
  const detailFileList = computed(() => detail.value ? flatten(detail.value.files) : []);
  // Zip installation lives beside the component installer, not inside it.
  const zipVisible = ref(false); const zipFile = ref<File | null>(null); const zipInstalling = ref(false); const zipError = ref(""); const zipWarnings = ref<string[]>([]);
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
    { value: "wechat_pad", label: "WeChatPadPro", desc: "微信", icon: wechatIcon },
  ] as const;
  /** Installed by a dedicated backend installer instead of the compose/binary pipeline. */
  function isSpecialComponent(type: string) { return type === "sqlite" || type === "wechat_pad"; }
  /** Embedded components install silently: no method choice, no configuration. */
  function isEmbeddedComponent(type: string) { return type === "sqlite"; }
  /** WeChatPadPro only ships as a Docker image, so the native install methods do not apply. */
  function isDockerOnlyComponent(type: string) { return type === "wechat_pad"; }
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
  const columns = [{ colKey: "name", title: "插件名称" }, { colKey: "component_type", title: "组件", width: 130 }, { colKey: "installation_method", title: "安装方式", width: 150 }, { colKey: "status", title: "状态", width: 110 }, { colKey: "connection_ids", title: "关联连接", width: 100 }, { colKey: "actions", title: "操作", width: 500 }];
  /** Plugins under `plugins/` are directory-owned: the row is informational and cannot be
   *  installed, toggled or uninstalled from here. */
  function isScriptPlugin(plugin: Pick<PluginRecord, "installation_method">) { return plugin.installation_method === "dynamic_script"; }
  function installMethodLabel(value: string) { return value === "dynamic_script" ? "动态脚本" : value; }
  /** Dynamic script plugins use a registry key as their component; show a readable label. */
  function componentLabel(plugin: PluginRecord) {
    if (!isScriptPlugin(plugin)) {
      return ({ wechat_pad: "WeChatPadPro" } as Record<string, string>)[plugin.component_type] ?? plugin.component_type;
    }
    return ({ web_search_engine: "搜索引擎", brain_tool: "智能体工具" } as Record<string, string>)[plugin.component_type] ?? plugin.component_type;
  }
  function scriptPluginDetail(plugin: PluginRecord): string {
    const description = plugin.extra_install_metadata?.description;
    return typeof description === "string" ? description : "";
  }
  /** The bot wxid bound by QR login, recorded in the plugin's connection-config snapshot. */
  function wechatWxid(plugin: PluginRecord): string {
    const config = plugin.extra_install_metadata?.connection_config as
      | { kind?: { wechat?: { self_wxid?: unknown } } }
      | undefined;
    const wxid = config?.kind?.wechat?.self_wxid;
    return typeof wxid === "string" && wxid.trim() ? wxid.trim() : "";
  }
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
    if (!isDockerOnlyComponent(form.component_type) && environment.value.binary_install_available) return "binary";
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
    const showProgress = !isSpecialComponent(form.component_type) && (form.install_method === "docker" || form.install_method === "binary");
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
      // Embedded components carry no install options; everything else — including the
      // docker-only special installer — uses the standard method and config fields.
      const result = await pluginsApi.install({ ...plugin, ...(isEmbeddedComponent(form.component_type) ? {} : { install_method, detailed_config: config }) });
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
  async function removePlugin(id: string) { try { const result = await pluginsApi.remove(id); await load(); if (result.uninstall_command) { uninstallCommand.value = result.uninstall_command; uninstallCommandVisible.value = true; } } catch (e) { error.value = String(e); } }

  // WeChat QR login dialog: fetch a bridge QR code, poll the scan result and stop at either a
  // confirmed login (persisted server-side) or an expired code the user can refresh.
  type WechatLoginPhase = "idle" | "waiting" | "scanned" | "logged_in" | "expired" | "error";
  const wechatLoginVisible = ref(false);
  const wechatLoginPlugin = ref<PluginRecord | null>(null);
  const wechatLoginLoading = ref(false);
  const wechatLoginPhase = ref<WechatLoginPhase>("idle");
  const wechatLoginError = ref("");
  const wechatLoginQrBase64 = ref("");
  const wechatLoginQrLink = ref("");
  const wechatLoginAccount = ref<{ wxid: string; nickname: string } | null>(null);
  const wechatLoginCountdown = ref(0);
  let wechatLoginAuthKey = "";
  let wechatLoginExpiresAt = 0;
  let wechatLoginAttempt = 0;
  let wechatLoginTimer: ReturnType<typeof setTimeout> | null = null;
  let wechatLoginCountdownTimer: ReturnType<typeof setInterval> | null = null;
  function stopWechatLoginLoop() {
    wechatLoginAttempt += 1;
    if (wechatLoginTimer !== null) { clearTimeout(wechatLoginTimer); wechatLoginTimer = null; }
    if (wechatLoginCountdownTimer !== null) { clearInterval(wechatLoginCountdownTimer); wechatLoginCountdownTimer = null; }
  }
  function openWechatLogin(plugin: PluginRecord) {
    stopWechatLoginLoop();
    wechatLoginPlugin.value = plugin;
    wechatLoginError.value = "";
    wechatLoginQrBase64.value = "";
    wechatLoginQrLink.value = "";
    wechatLoginAccount.value = null;
    wechatLoginPhase.value = "idle";
    wechatLoginVisible.value = true;
    void requestWechatQr();
  }
  function closeWechatLogin() {
    stopWechatLoginLoop();
    wechatLoginVisible.value = false;
    wechatLoginPlugin.value = null;
  }
  function startWechatLoginCountdown() {
    wechatLoginCountdown.value = Math.max(0, Math.round((wechatLoginExpiresAt - Date.now()) / 1000));
    wechatLoginCountdownTimer = setInterval(() => {
      wechatLoginCountdown.value = Math.max(0, Math.round((wechatLoginExpiresAt - Date.now()) / 1000));
      if (wechatLoginCountdown.value === 0 && wechatLoginPhase.value !== "logged_in") {
        wechatLoginPhase.value = "expired";
        stopWechatLoginLoop();
      }
    }, 1000);
  }
  async function requestWechatQr() {
    const plugin = wechatLoginPlugin.value;
    if (!plugin) return;
    stopWechatLoginLoop();
    const attempt = wechatLoginAttempt;
    wechatLoginLoading.value = true;
    wechatLoginError.value = "";
    wechatLoginPhase.value = "idle";
    try {
      const result = await pluginsApi.wechatLoginQr(plugin.id);
      if (attempt !== wechatLoginAttempt) return;
      wechatLoginAuthKey = result.auth_key;
      if (result.status === "logged_in") {
        wechatLoginPhase.value = "logged_in";
        wechatLoginAccount.value = { wxid: result.wxid ?? "", nickname: result.nickname ?? "" };
        return;
      }
      wechatLoginQrBase64.value = result.qr_base64 ?? "";
      wechatLoginQrLink.value = result.qr_link ?? "";
      wechatLoginExpiresAt = Date.now() + (result.expires_in_secs ?? 290) * 1000;
      wechatLoginPhase.value = "waiting";
      startWechatLoginCountdown();
      pollWechatLoginStatus(attempt);
    } catch (e) {
      if (attempt !== wechatLoginAttempt) return;
      wechatLoginPhase.value = "error";
      wechatLoginError.value = String(e);
    } finally {
      if (attempt === wechatLoginAttempt) wechatLoginLoading.value = false;
    }
  }
  function pollWechatLoginStatus(attempt: number) {
    wechatLoginTimer = setTimeout(async () => {
      if (attempt !== wechatLoginAttempt || !wechatLoginPlugin.value) return;
      let result: WechatLoginStatusResult | null = null;
      try {
        result = await pluginsApi.wechatLoginStatus(wechatLoginPlugin.value.id, wechatLoginAuthKey);
        if (attempt !== wechatLoginAttempt) return;
        if (result.status === "logged_in") {
          wechatLoginPhase.value = "logged_in";
          wechatLoginAccount.value = { wxid: result.wxid ?? "", nickname: result.nickname ?? "" };
          stopWechatLoginLoop();
          await load();
          return;
        }
        if (result.status === "scanned") wechatLoginPhase.value = "scanned";
        if (result.status === "expired") {
          wechatLoginPhase.value = "expired";
          stopWechatLoginLoop();
          return;
        }
        if (result.status === "error" && result.message) wechatLoginError.value = result.message;
      } catch (e) {
        if (attempt !== wechatLoginAttempt) return;
        wechatLoginError.value = String(e);
      }
      if (Date.now() >= wechatLoginExpiresAt) {
        wechatLoginPhase.value = "expired";
        stopWechatLoginLoop();
        return;
      }
      pollWechatLoginStatus(attempt);
    }, 3000);
  }
  function wechatLoginStatusLabel(): string {
    switch (wechatLoginPhase.value) {
      case "idle": return "正在获取二维码...";
      case "waiting": return `请用微信扫描二维码（${wechatLoginCountdown.value} 秒后过期）`;
      case "scanned": return "已扫码，请在手机上确认登录";
      case "expired": return "二维码已过期，请刷新后重扫";
      case "error": return wechatLoginError.value || "登录流程出错";
      case "logged_in": {
        const account = wechatLoginAccount.value;
        if (!account) return "登录成功";
        const name = account.nickname || account.wxid;
        return account.wxid && account.nickname ? `登录成功：${account.nickname}（${account.wxid}）` : `登录成功：${name}`;
      }
    }
  }
  onMounted(() => { void load(); if (route.query.install === "1") openInstall(); });
  onBeforeUnmount(() => {
    installAttempt += 1;
    clearInstallationSideEffects();
    stopWechatLoginLoop();
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
    isEmbeddedComponent,
    isDockerOnlyComponent,
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
    isScriptPlugin,
    installMethodLabel,
  componentLabel,
  scriptPluginDetail,
  wechatWxid,
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
    detailVisible,
    detailLoading,
    detail,
    detailError,
    sourceOpen,
    activeFile,
    fileContent,
    fileLoading,
    fileError,
    detailFileList,
    openDetail,
    previewFile,
    removeScriptPlugin,
    zipVisible,
    zipFile,
    zipInstalling,
    zipError,
    zipWarnings,
  openZipInstall,
  selectZipFile,
  installZip,
  wechatLoginVisible,
  wechatLoginLoading,
  wechatLoginPhase,
  wechatLoginError,
  wechatLoginQrBase64,
  wechatLoginQrLink,
  wechatLoginAccount,
  openWechatLogin,
  closeWechatLogin,
  requestWechatQr,
  wechatLoginStatusLabel,
  };
}
