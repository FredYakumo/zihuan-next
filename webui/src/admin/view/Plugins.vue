<template>
  <section class="page plugins-page">
    <AdminPageHeader title="插件" />
    <t-card title="已安装插件" bordered header-bordered>
      <template #actions><t-button theme="primary" @click="openInstall">安装插件</t-button></template>
      <t-table :data="plugins" :columns="columns" row-key="id" :loading="loading" :pagination="false" size="small">
        <template #status="{ row }"><t-tag :theme="statusTheme(row.status)" variant="light">{{ statusLabel(row.status) }}</t-tag></template>
        <template #connection_ids="{ row }"><span>{{ row.connection_ids?.length ?? 0 }} 个连接</span></template>
        <template #actions="{ row }"><t-space size="small"><t-button v-if="installCommand(row)" variant="text" @click="copyPluginValue(installCommand(row)!, '安装命令')"><template #icon><CopyIcon /></template>复制安装命令</t-button><t-button v-if="connectionConfigJson(row)" variant="text" @click="copyPluginValue(connectionConfigJson(row)!, '连接 JSON')"><template #icon><CopyIcon /></template>复制连接 JSON</t-button><t-button v-if="row.status === 'installed'" variant="text" @click="toggle(row)">停用</t-button><t-button v-else-if="row.status === 'disabled'" variant="text" @click="toggle(row)">启用</t-button><t-popconfirm content="确认卸载插件并删除关联连接吗？" @confirm="removePlugin(row.id)"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm></t-space></template>
      </t-table>
    </t-card>
    <t-dialog
      :visible="installVisible"
      width="720px"
      :close-on-overlay-click="false"
      @close="requestCloseInstall"
    >
      <template #header>
        <div class="dialog-header">
          <t-button v-if="installStep === 'config'" variant="text" size="small" class="dialog-back" @click="backFromConfig"><ChevronLeftIcon /></t-button>
          <span class="dialog-title">{{ installStep === "progress" ? "安装进度" : "安装插件" }}</span>
        </div>
      </template>
      <div v-if="installStep === 'picker'" class="component-picker">
        <p class="component-picker-title">选择组件类型</p>
        <div class="component-grid">
          <button v-for="item in componentOptions" :key="item.value" type="button" :class="['component-card', { active: form.component_type === item.value }]" @click="selectComponent(item.value)">
            <img class="component-card-icon" :src="item.icon" :alt="item.label" />
            <strong class="component-card-name">{{ item.label }}</strong>
            <span class="component-card-desc">{{ item.desc }}</span>
          </button>
        </div>
      </div>
      <div v-else-if="installStep === 'config'" class="install-config">
      <div class="selected-component-bar">
        <img class="selected-component-icon" :src="selectedComponent.icon" :alt="selectedComponent.label" />
        <div class="selected-component-info">
          <strong>{{ selectedComponent.label }}</strong>
          <span>{{ selectedComponent.desc }}</span>
        </div>
      </div>
      <t-form label-align="top">
        <div class="form-grid">
          <t-form-item label="插件名称" required><t-input v-model="form.name" /></t-form-item>
          <t-form-item v-if="form.component_type !== 'sqlite'" label="安装方式" required class="install-method-item">
            <div class="install-method">
              <span v-if="environmentLoading" class="install-method-status">正在检测本机安装能力...</span>
              <template v-else>
                <label :class="{ unavailable: !dockerSupported }" :title="dockerUnsupportedReason"><input v-model="form.install_method" type="radio" value="docker" :disabled="!dockerSupported" /> 本机 Docker <small v-if="!dockerSupported">（不可用）</small></label>
                <label :class="{ unavailable: !binarySupported }" :title="binaryUnsupportedReason"><input v-model="form.install_method" type="radio" value="binary" :disabled="!binarySupported" /> 本机 native 程序 <small v-if="!binarySupported">（不可用）</small></label>
                <label><input v-model="form.install_method" type="radio" value="command_docker" /> Docker 安装命令</label>
                <label><input v-model="form.install_method" type="radio" value="command_binary" /> native 程序安装命令</label>
              </template>
            </div>
          </t-form-item>
        </div>
        <template v-if="!environmentLoading">
        <div v-if="form.component_type !== 'sqlite'" class="global-config">
          <label><input v-model="config.expose_public_access" type="checkbox" /> 暴露公网访问</label>
          <label v-if="config.expose_public_access"><input v-model="config.use_target_machine_address" type="checkbox" /> 使用目标机器地址</label>
          <SetupField v-if="config.expose_public_access && config.use_target_machine_address" label="目标机器地址（可选）"><input v-model="config.target_machine_address" placeholder="例如 203.0.113.10" /></SetupField>
        </div>
        <div v-if="form.component_type === 'mysql'" class="form-grid">
          <SetupDeploymentFields v-model="config.relational.deployment" />
          <SetupField label="主机"><input v-model="config.relational.host" /></SetupField>
          <SetupField label="端口"><input v-model.number="config.relational.deployment.port" type="number" min="1" /></SetupField>
          <SetupField label="用户名"><input v-model="config.relational.username" /></SetupField>
          <SetupField label="密码"><SetupCredentialInput v-model="config.relational.password" /></SetupField>
          <SetupField label="数据库"><input v-model="config.relational.database" /></SetupField>
          <SetupField label="最大连接数"><input v-model.number="config.relational.max_connections" type="number" min="1" /></SetupField>
          <SetupField label="获取连接超时（秒）"><input v-model.number="config.relational.acquire_timeout_secs" type="number" min="1" /></SetupField>
        </div>
        <div v-else-if="form.component_type === 'redis'" class="form-grid">
          <SetupDeploymentFields v-model="config.redis.deployment" />
          <SetupField label="Redis URL"><input v-model="config.redis.url" /></SetupField>
          <SetupField label="用户名"><input v-model="config.redis.username" /></SetupField>
          <SetupField label="密码"><SetupCredentialInput v-model="config.redis.password" /></SetupField>
        </div>
        <div v-else-if="form.component_type === 'rustfs'" class="form-grid">
          <SetupDeploymentFields v-model="config.rustfs.deployment" />
          <SetupField label="Endpoint"><input v-model="config.rustfs.endpoint" /></SetupField>
          <SetupField label="Bucket"><input v-model="config.rustfs.bucket" /></SetupField>
          <SetupField label="Region"><input v-model="config.rustfs.region" /></SetupField>
          <SetupField label="Access Key"><SetupCredentialInput v-model="config.rustfs.access_key" input-type="text" /></SetupField>
          <SetupField label="Secret Key"><SetupCredentialInput v-model="config.rustfs.secret_key" /></SetupField>
          <label><input v-model="config.rustfs.path_style" type="checkbox" /> 使用 path-style</label>
        </div>
        <div v-else-if="form.component_type === 'weaviate' || form.component_type === 'elasticsearch'" class="form-grid">
          <SetupDeploymentFields v-model="config.search.deployment" />
          <SetupField label="Base URL"><input v-model="config.search.base_url" /></SetupField>
          <SetupField v-if="form.component_type === 'elasticsearch'" label="用户名"><input v-model="config.search.username" disabled /></SetupField>
          <SetupField :label="form.component_type === 'elasticsearch' ? '密码' : 'API Key'"><SetupCredentialInput v-if="form.component_type === 'elasticsearch'" v-model="config.search.password" /><SetupCredentialInput v-else v-model="config.search.api_key" /></SetupField>
          <SetupField label="向量维度"><input v-model.number="config.search.vector_dimensions" type="number" min="1" /></SetupField>
        </div>
        <t-alert v-if="error" theme="error" :message="error" />
        </template>
      </t-form>
      </div>
      <InstallationProgress
        v-else
        :task-id="taskId"
        :logs="progress"
        :error="error || null"
        title="正在安装插件..."
        subtitle="请稍候，系统正在执行安装和连接配置"
        back-label="返回配置"
        @retry="install"
        @back="backFromProgress"
      />
      <template v-if="installStep !== 'progress'" #footer>
        <div class="dialog-footer">
          <t-button variant="outline" @click="requestCloseInstall">取消</t-button>
          <t-button theme="primary" :loading="saving" :disabled="installStep !== 'config' || (form.component_type !== 'sqlite' && environmentLoading)" @click="install">开始安装</t-button>
        </div>
      </template>
    </t-dialog>
    <t-dialog
      v-model:visible="exitConfirmVisible"
      header="退出安装界面？"
      confirm-btn="退出"
      cancel-btn="继续等待"
      :close-on-overlay-click="false"
      @confirm="confirmExitInstallation"
    >
      安装任务会继续在后台运行。退出后可以在插件列表中查看当前状态，请勿重复提交安装。
    </t-dialog>
    <InstallationSuccessDialog
      :visible="successVisible"
      :install-command="successInstallCommand"
      :connection-config="successConnectionConfig"
      @confirm="confirmInstallationSuccess"
    />
    <t-dialog v-model:visible="uninstallCommandVisible" header="卸载命令" :confirm-btn="null" cancel-btn="关闭">
      <div class="plugin-command-dialog">
        <t-button variant="text" shape="square" @click="copyPluginValue(uninstallCommand, '卸载命令')"><template #icon><CopyIcon /></template></t-button>
        <t-textarea :value="uninstallCommand" readonly autosize />
      </div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { ChevronLeftIcon, CopyIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import InstallationSuccessDialog from "../components/InstallationSuccessDialog.vue";
import { pluginsApi, setup as setupApi, type DetailedSetupConfig, type EnvironmentInfo, type PluginRecord, type SetupProgressEvent } from "../../api/client";
import InstallationProgress from "../setup/InstallationProgress.vue";
import SetupCredentialInput from "../setup/SetupCredentialInput.vue";
import SetupDeploymentFields from "../setup/SetupDeploymentFields.vue";
import SetupField from "../setup/SetupField.vue";
import elasticsearchIcon from "../../assets/icons/elasticsearch.svg";
import mysqlIcon from "../../assets/icons/mysql.svg";
import redisIcon from "../../assets/icons/redis.svg";
import rustfsIcon from "../../assets/icons/rustfs.svg";
import sqliteIcon from "../../assets/icons/sqlite.svg";
import weaviateIcon from "../../assets/icons/weaviate.svg";

const route = useRoute();
type InstallStep = "picker" | "config" | "progress";

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
type PluginInstallMethod = "docker" | "binary" | "command_docker" | "command_binary";
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
      }, () => {
        if (attempt !== installAttempt || !saving.value) return;
        clearInstallationSideEffects();
        saving.value = false;
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
function installCommand(plugin: PluginRecord): string | null { return metadataString(plugin, "install_command"); }
function connectionConfigJson(plugin: PluginRecord): string | null { const value = plugin.extra_install_metadata?.connection_config; return value == null ? null : JSON.stringify(value, null, 2); }
async function copyPluginValue(value: string, label: string) { try { await navigator.clipboard.writeText(value); } catch (e) { error.value = `复制${label}失败：${String(e)}`; } }
async function removePlugin(id: string) { try { const result = await pluginsApi.remove(id); await load(); if (result.uninstall_command) { uninstallCommand.value = result.uninstall_command; uninstallCommandVisible.value = true; } } catch (e) { error.value = String(e); } }
onMounted(() => { void load(); if (route.query.install === "1") openInstall(); });
onBeforeUnmount(() => {
  installAttempt += 1;
  clearInstallationSideEffects();
});
</script>

<style scoped lang="scss">
.component-picker-title { margin: 0 0 12px; font-weight: 600; color: var(--admin-ink); }
.component-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
.component-card { display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 18px 12px; border: 1px solid var(--admin-border); border-radius: 8px; background: var(--admin-bg-panel); color: var(--admin-ink); cursor: pointer; transition: border-color 0.15s ease;
  &:hover, &:focus-visible { border-color: var(--admin-accent); outline: none; }
  &.active { border-color: var(--admin-accent); box-shadow: inset 0 0 0 1px var(--admin-accent); }
}
.component-card-icon { width: 40px; height: 40px; object-fit: contain; }
.component-card-name { font-size: 14px; }
.component-card-desc { font-size: 12px; color: var(--admin-muted); text-align: center; }
.selected-component-bar { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; padding: 10px 12px; border: 1px solid var(--admin-border); border-radius: 8px; background: var(--admin-bg-panel); }
.dialog-header { display: flex; align-items: center; gap: 4px; }
.dialog-back { margin-left: -8px; }
.dialog-footer { display: flex; justify-content: flex-end; gap: 8px; }
.selected-component-icon { width: 28px; height: 28px; object-fit: contain; }
.selected-component-info { display: flex; flex-direction: column; flex: 1; line-height: 1.4;
  span { font-size: 12px; color: var(--admin-muted); }
}
.form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.install-method-item { grid-column: 1 / -1; }
.install-method { display: flex; flex-wrap: wrap; gap: 6px 18px; align-items: center;
  label { display: inline-flex; align-items: center; gap: 4px; cursor: pointer; }
  label.unavailable { color: var(--admin-muted); cursor: not-allowed; }
  small { color: var(--admin-muted); }
}
.install-method-status { color: var(--admin-muted); }
.global-config { display: flex; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.progress-line { color: var(--admin-muted); }
textarea { width: 100%; min-height: 140px; font-family: ui-monospace, monospace; }
.plugin-command-dialog { display: grid; gap: 8px; }
.plugin-command-dialog > :first-child { justify-self: end; }
</style>
