import { computed, ref, watch } from "vue";

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

export interface RetrievalDatabaseDialogProps {
  visible: boolean;
}

export interface RetrievalDatabaseDialogEmit {
  (event: "update:visible", visible: boolean): void;
  (event: "created", connection: ConnectionConfig): void;
  (event: "clipboard-import"): void;
  (event: "file-change", value: Event): void;
  (event: "create-page"): void;
}

export function useRetrievalDatabaseDialog(
  props: RetrievalDatabaseDialogProps,
  emit: RetrievalDatabaseDialogEmit,
) {
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
  // The installed database usually runs on another machine, so the address in the
  // generated connection config must be user supplied instead of the loopback
  // default from the template.
  const serverHost = ref("127.0.0.1");
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
  const resolvedBaseUrl = computed(() => {
    // Command installs run on a target host, so the connection config must follow
    // the address the user typed instead of the loopback default.
    const host = (serverHost.value.trim() || "127.0.0.1").replace(/\/+$/, "");
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(host)) {
      return host;
    }
    // Accept "host", "host:port" and "[::1]:port" without doubling the port.
    const hasPort = /^\[[^\]]+\]:\d+$/.test(host) || /^[^:]+:\d+$/.test(host);
    return `http://${host}${hasPort ? "" : `:${search.value.deployment.port}`}`;
  });
  const connectionsJson = computed(() => JSON.stringify(commandResult.value?.connections ?? [], null, 2));

  watch(installOption, (option) => {
    // Local installs always listen on this machine; a command install targets a
    // different host, so start empty and require the user to fill it in.
    serverHost.value = option.startsWith("local_") ? "127.0.0.1" : "";
  });

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
    serverHost.value = installOption.value.startsWith("local_") ? "127.0.0.1" : "";
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
    return { ...search.value, base_url: resolvedBaseUrl.value, enabled: true, source: "install" };
  }

  function requireServerAddress(): boolean {
    if (isLocalOption.value) return true;
    if (serverHost.value.trim()) return true;
    installError.value = "请填写安装数据库的目标服务器地址，生成的连接配置会使用该地址";
    return false;
  }

  async function startInstall() {
    if (installing.value) return;
    if (!requireServerAddress()) return;
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
    if (!requireServerAddress()) return;
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
    const connection = commandResult.value?.connections.find(
      (item) => item.kind.type === "weaviate" || item.kind.type === "elasticsearch",
    );
    if (!connection || importingCommand.value) return;
    importingCommand.value = true;
    try {
      const created = await system.connections.create({
        name: connection.name,
        enabled: connection.enabled,
        kind: connection.kind,
        allow_create_collection: true,
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

  return {
    mode,
    searchType,
    search,
    installOption,
    environmentLoading,
    installError,
    installLogs,
    commandResult,
    copied,
    fileInput,
    serverHost,
    dockerSupported,
    dockerUnsupportedReason,
    localBinarySupported,
    localBinaryDisabledReason,
    isLocalOption,
    resolvedBaseUrl,
    connectionsJson,
    installing,
    generating,
    importingCommand,
    startInstall,
    generateCommand,
    resetInstallState,
    importCommandConnection,
    copyText,
  };
}
