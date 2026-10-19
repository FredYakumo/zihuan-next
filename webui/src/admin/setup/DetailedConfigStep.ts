import { computed, onMounted, ref, watch, type Ref } from "vue";

import { setup as setupApi, type DetailedSetupConfig, type DetailedSetupInstallMethod, type EnvironmentInfo } from "../../api/client";

export type DetailedInstallOption = "local_docker" | "local_binary" | "command_docker" | "command_binary";

export function useDetailedConfigStep(model: Ref<DetailedSetupConfig>) {
  const environmentLoading = ref(true);
  const environment = ref<EnvironmentInfo>({
    os: "", os_detail: "", docker_available: false, docker_compose_available: false,
    binary_install_available: false, binary_install_reason: null, wsl_available: null, wsl_docker_available: null,
    cuda_version: null, compiler_version: null, proxy: null, services: [],
  });
  const dockerSupported = computed(() => environment.value.docker_compose_available);
  const dockerUnsupportedReason = computed(() => "Docker Compose 不可用，请安装并启动 Docker Desktop 或 Docker Compose");
  const selectedInstallOption = ref<DetailedInstallOption>(
    model.value.install_method === "docker" ? "command_docker" : "command_binary",
  );
  const isLocalInstall = computed(() => selectedInstallOption.value.startsWith("local_"));
  const isInstalledElasticsearch = computed(() =>
    model.value.search.type === "elasticsearch" && model.value.search.source === "install",
  );
  const isInstalledWeaviate = computed(() =>
    model.value.search.type === "weaviate" && model.value.search.source === "install",
  );
  const isSearchAuthLocked = computed(() => isInstalledElasticsearch.value || isInstalledWeaviate.value);
  const usesPasswordAuth = computed(() => model.value.search.auth_method === "password");

  onMounted(async () => {
    try {
      environment.value = await setupApi.getEnvironment();
      selectedInstallOption.value = getPreferredInstallOption();
    } catch (error) {
      console.error("Failed to detect setup environment", error);
    } finally {
      environmentLoading.value = false;
    }
  });

  watch(selectedInstallOption, (option) => {
    const installMethod: DetailedSetupInstallMethod = option.endsWith("docker") ? "docker" : "binary";
    model.value.install_method = installMethod;
  });

  watch(
    () => [model.value.search.type, model.value.search.source],
    () => {
      if (isInstalledElasticsearch.value) {
        model.value.search.auth_method = "password";
        model.value.search.username = "elastic";
      } else if (isInstalledWeaviate.value) {
        model.value.search.auth_method = "api_key";
      }
      clearInactiveSearchCredentials();
    },
    { immediate: true },
  );

  watch(() => model.value.search.auth_method, clearInactiveSearchCredentials);

  function getPreferredInstallOption(): DetailedInstallOption {
    if (dockerSupported.value) {
      return "local_docker";
    }
    if (environment.value.binary_install_available) {
      return "local_binary";
    }
    return "command_docker";
  }

  function setSearchType(type: "weaviate" | "elasticsearch") {
    model.value.search.type = type;
    if (type === "elasticsearch") {
      model.value.search.deployment = { ...model.value.search.deployment, image: "docker.elastic.co/elasticsearch/elasticsearch:8.17.0", port: 9200, data_dir: "./data/zihuan-elasticsearch", container_name: "zihuan-elasticsearch" };
      model.value.search.base_url = "http://127.0.0.1:9200";
      model.value.search.username = "elastic";
      model.value.search.api_key = null;
      model.value.search.auth_method = "password";
    } else {
      model.value.search.deployment = { ...model.value.search.deployment, image: "cr.weaviate.io/semitechnologies/weaviate:1.30.5", port: 8080, data_dir: "./data/zihuan-weaviate", container_name: "zihuan-weaviate" };
      model.value.search.base_url = "http://127.0.0.1:8080";
      model.value.search.auth_method = "api_key";
    }
  }

  function clearInactiveSearchCredentials() {
    if (model.value.search.auth_method === "password") {
      model.value.search.api_key = null;
      return;
    }
    model.value.search.username = null;
    model.value.search.password = null;
  }

  return {
    environmentLoading,
    environment,
    dockerSupported,
    dockerUnsupportedReason,
    selectedInstallOption,
    isLocalInstall,
    isInstalledElasticsearch,
    isSearchAuthLocked,
    usesPasswordAuth,
    setSearchType,
  };
}
