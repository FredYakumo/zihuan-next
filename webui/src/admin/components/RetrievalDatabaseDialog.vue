<template>
  <t-dialog
    :visible="visible"
    header="新增检索数据库"
    :confirm-btn="null"
    cancel-btn="关闭"
    :close-on-overlay-click="false"
    width="760px"
    @update:visible="emit('update:visible', $event)"
  >
    <div class="retrieval-database-dialog">
      <div class="mode-switch">
        <label><input v-model="mode" type="radio" value="install" /> 安装新数据库</label>
        <label><input v-model="mode" type="radio" value="existing" /> 使用现有配置</label>
      </div>

      <template v-if="mode === 'install'">
        <p v-if="environmentLoading" class="install-hint">正在检测本机安装能力...</p>
        <template v-else>
          <div class="choice-row">
            <span class="choice-title">数据库类型</span>
            <label><input v-model="searchType" type="radio" value="weaviate" /> Weaviate</label>
            <label><input v-model="searchType" type="radio" value="elasticsearch" /> Elasticsearch</label>
          </div>
          <div class="choice-row">
            <span class="choice-title">安装方式</span>
            <label :class="{ unavailable: !dockerSupported }" :title="dockerUnsupportedReason">
              <input v-model="installOption" type="radio" value="local_docker" :disabled="!dockerSupported" />
              本机 Docker <small v-if="!dockerSupported">（本机不支持）</small>
            </label>
            <label
              :class="{ unavailable: !localBinarySupported }"
              :title="localBinaryDisabledReason ?? ''"
            >
              <input v-model="installOption" type="radio" value="local_binary" :disabled="!localBinarySupported" />
              本机二进制 <small v-if="localBinaryDisabledReason">（{{ localBinaryDisabledReason }}）</small>
            </label>
            <label>
              <input v-model="installOption" type="radio" value="command_docker" />
              安装命令（Docker）
            </label>
            <label>
              <input v-model="installOption" type="radio" value="command_binary" />
              安装命令（二进制）
            </label>
          </div>

          <div class="install-grid">
            <label class="install-grid-item">
              <span>镜像</span>
              <input v-model="search.deployment.image" />
            </label>
            <label class="install-grid-item">
              <span>端口</span>
              <input v-model.number="search.deployment.port" type="number" min="1" />
            </label>
            <label class="install-grid-item">
              <span>数据目录</span>
              <input v-model="search.deployment.data_dir" />
            </label>
            <label class="install-grid-item">
              <span>容器名</span>
              <input v-model="search.deployment.container_name" />
            </label>
            <label class="install-grid-item">
              <span>Base URL</span>
              <input v-model="search.base_url" />
            </label>
            <label v-if="search.auth_method === 'api_key'" class="install-grid-item">
              <span>API Key</span>
              <input v-model="search.api_key" />
            </label>
            <template v-else>
              <label class="install-grid-item">
                <span>用户名</span>
                <input v-model="search.username" disabled />
              </label>
              <label class="install-grid-item">
                <span>密码</span>
                <input v-model="search.password" type="password" />
              </label>
            </template>
            <label v-if="search.type === 'elasticsearch'" class="install-grid-item">
              <span>向量维度</span>
              <input v-model.number="search.vector_dimensions" type="number" min="1" />
            </label>
          </div>

          <p v-if="installError" class="install-error">{{ installError }}</p>

          <div class="install-actions">
            <t-button v-if="isLocalOption" theme="primary" :loading="installing" @click="startInstall">
              {{ installing ? "安装中..." : "开始安装" }}
            </t-button>
            <t-button v-else theme="primary" :loading="generating" @click="generateCommand">
              生成安装命令
            </t-button>
            <t-button v-if="installError || installLogs.length > 0" variant="text" :disabled="installing" @click="resetInstallState">
              重置
            </t-button>
          </div>

          <div v-if="installLogs.length > 0" class="install-progress">
            <div v-for="(line, index) in installLogs" :key="index" class="install-progress-line">{{ line }}</div>
          </div>

          <template v-if="commandResult">
            <section class="command-output">
              <div class="command-output-header">
                <span>安装命令</span>
                <t-button variant="text" size="small" @click="copyText(commandResult.install_command, 'command')">
                  <CopyIcon /> {{ copied === "command" ? "已复制" : "复制" }}
                </t-button>
              </div>
              <textarea readonly :value="commandResult.install_command" aria-label="安装命令" />
            </section>
            <section class="command-output">
              <div class="command-output-header">
                <span>连接配置 JSON</span>
                <t-button variant="text" size="small" @click="copyText(connectionsJson, 'connections')">
                  <CopyIcon /> {{ copied === "connections" ? "已复制" : "复制" }}
                </t-button>
              </div>
              <textarea readonly :value="connectionsJson" aria-label="连接配置 JSON" />
            </section>
            <div class="install-actions">
              <t-button theme="primary" :loading="importingCommand" @click="importCommandConnection">
                已完成安装，导入连接
              </t-button>
            </div>
          </template>
        </template>
      </template>

      <template v-else>
        <div class="existing-actions">
          <t-button block theme="primary" @click="emit('create-page')">前往连接页创建</t-button>
          <t-button block variant="outline" @click="emit('clipboard-import')">从剪贴板导入</t-button>
          <t-button block variant="outline" @click="fileInput?.click()">从 JSON 导入</t-button>
          <input
            ref="fileInput"
            type="file"
            accept=".json,application/json"
            class="existing-file-input"
            @change="emit('file-change', $event)"
          />
        </div>
      </template>
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { CopyIcon } from "tdesign-icons-vue-next";

import {
  setup as setupApi,
  system,
  type ConnectionConfig,
  type DetailedInstallCommandResult,
  type DetailedSearchSetupConfig,
  type DetailedSearchType,
  type DetailedSetupInstallMethod,
  type EnvironmentInfo,
} from "../../api/client";

const RETRIEVAL_DEFAULTS: Record<DetailedSearchType, DetailedSearchSetupConfig> = {
  weaviate: {
    enabled: true,
    source: "install",
    type: "weaviate",
    deployment: {
      image: "cr.weaviate.io/semitechnologies/weaviate:1.30.5",
      port: 8080,
      data_dir: "./data/zihuan-weaviate",
      container_name: "zihuan-weaviate",
      restart_policy: "unless-stopped",
    },
    base_url: "http://127.0.0.1:8080",
    username: null,
    password: null,
    api_key: "zihuan-weaviate-api-key",
    auth_method: "api_key",
    vector_dimensions: 1024,
  },
  elasticsearch: {
    enabled: true,
    source: "install",
    type: "elasticsearch",
    deployment: {
      image: "docker.elastic.co/elasticsearch/elasticsearch:8.17.0",
      port: 9200,
      data_dir: "./data/zihuan-elasticsearch",
      container_name: "zihuan-elasticsearch",
      restart_policy: "unless-stopped",
    },
    base_url: "http://127.0.0.1:9200",
    username: "elastic",
    password: "",
    api_key: null,
    auth_method: "password",
    vector_dimensions: 1024,
  },
};

type InstallOption = "local_docker" | "local_binary" | "command_docker" | "command_binary";

const props = defineProps<{ visible: boolean }>();

const emit = defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "created", connection: ConnectionConfig): void;
  (event: "clipboard-import"): void;
  (event: "file-change", value: Event): void;
  (event: "create-page"): void;
}>();

const mode = ref<"install" | "existing">("install");
const searchType = ref<DetailedSearchType>("weaviate");
const search = ref<DetailedSearchSetupConfig>({ ...RETRIEVAL_DEFAULTS.weaviate });
const installOption = ref<InstallOption>("local_docker");
const environment = ref<EnvironmentInfo | null>(null);
const environmentLoading = ref(false);
const installing = ref(false);
const generating = ref(false);
const importingCommand = ref(false);
const installError = ref<string | null>(null);
const installLogs = ref<string[]>([]);
const commandResult = ref<DetailedInstallCommandResult | null>(null);
const copied = ref<"command" | "connections" | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);
let stopProgress: (() => void) | null = null;

const dockerSupported = computed(() => environment.value?.docker_compose_available ?? false);
const dockerUnsupportedReason = "Docker Compose 不可用，请安装并启动 Docker Desktop 或 Docker Compose";
const localBinarySupported = computed(() => {
  const env = environment.value;
  if (!env?.binary_install_available) return false;
  return !(searchType.value === "weaviate" && env.os === "windows");
});
const localBinaryDisabledReason = computed(() => {
  const env = environment.value;
  if (!env) return null;
  if (!env.binary_install_available) return env.binary_install_reason ?? "本机不支持二进制安装";
  if (searchType.value === "weaviate" && env.os === "windows") return "Weaviate 官方未提供 Windows 二进制";
  return null;
});
const isLocalOption = computed(() => installOption.value.startsWith("local_"));
const installMethod = computed<DetailedSetupInstallMethod>(() =>
  installOption.value.endsWith("docker") ? "docker" : "binary",
);
const connectionsJson = computed(() => JSON.stringify(commandResult.value?.connections ?? [], null, 2));

watch(searchType, (type) => {
  search.value = { ...RETRIEVAL_DEFAULTS[type] };
  commandResult.value = null;
});

watch(localBinarySupported, (supported) => {
  if (!supported && installOption.value === "local_binary") {
    installOption.value = dockerSupported.value ? "local_docker" : "command_docker";
  }
});

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      resetState();
      void loadEnvironment();
    } else {
      stopProgress?.();
      stopProgress = null;
    }
  },
);

function resetState() {
  mode.value = "install";
  searchType.value = "weaviate";
  search.value = { ...RETRIEVAL_DEFAULTS.weaviate };
  installOption.value = dockerSupported.value ? "local_docker" : "command_docker";
  installError.value = null;
  installLogs.value = [];
  commandResult.value = null;
}

async function loadEnvironment() {
  environmentLoading.value = true;
  try {
    environment.value = await setupApi.getEnvironment();
    if (!dockerSupported.value && localBinarySupported.value) {
      installOption.value = "local_binary";
    } else if (!dockerSupported.value) {
      installOption.value = "command_docker";
    }
  } catch (error) {
    console.warn("Failed to detect setup environment", error);
  } finally {
    environmentLoading.value = false;
  }
}

function searchPayload(): DetailedSearchSetupConfig {
  return { ...search.value, enabled: true, source: "install" };
}

async function startInstall() {
  if (installing.value) return;
  installing.value = true;
  installError.value = null;
  installLogs.value = [];
  try {
    const response = await system.retrievalDatabases.install({
      install_method: installMethod.value,
      search: searchPayload(),
    });
    stopProgress?.();
    stopProgress = setupApi.streamProgress(
      response.task_id,
      (event) => {
        installLogs.value.push(event.message);
        if (event.status === "error") {
          installing.value = false;
          installError.value = event.error ?? event.message;
        }
        if (event.step === "finished" && event.connection) {
          installing.value = false;
          stopProgress?.();
          stopProgress = null;
          emit("created", event.connection);
          emit("update:visible", false);
        }
      },
      () => {
        installing.value = false;
      },
    );
  } catch (error) {
    installing.value = false;
    installError.value = error instanceof Error ? error.message : String(error);
  }
}

function resetInstallState() {
  if (installing.value) return;
  installError.value = null;
  installLogs.value = [];
}

async function generateCommand() {
  if (generating.value) return;
  generating.value = true;
  installError.value = null;
  commandResult.value = null;
  try {
    commandResult.value = await system.retrievalDatabases.generateInstallCommand({
      install_method: installMethod.value,
      search: searchPayload(),
    });
  } catch (error) {
    installError.value = error instanceof Error ? error.message : String(error);
  } finally {
    generating.value = false;
  }
}

async function importCommandConnection() {
  const connection = commandResult.value?.connections[0];
  if (!connection || importingCommand.value) return;
  importingCommand.value = true;
  try {
    const created = await system.connections.create({
      name: connection.name,
      enabled: connection.enabled,
      kind: connection.kind,
    });
    emit("created", created);
    emit("update:visible", false);
  } catch (error) {
    installError.value = `连接导入失败：${error instanceof Error ? error.message : String(error)}`;
  } finally {
    importingCommand.value = false;
  }
}

async function copyText(value: string, target: "command" | "connections") {
  try {
    await navigator.clipboard.writeText(value);
    copied.value = target;
    window.setTimeout(() => {
      if (copied.value === target) copied.value = null;
    }, 1600);
  } catch (error) {
    installError.value = `复制失败：${error instanceof Error ? error.message : String(error)}`;
  }
}
</script>

<style scoped lang="scss">
@use "./retrieval-database-dialog" as *;
</style>
