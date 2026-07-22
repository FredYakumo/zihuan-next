import { computed, ref } from "vue";
import { useRouter } from "vue-router";

import {
  setup as setupApi,
  type ImsBotAdapterSetupConfig,
  type LlmSetupConfig,
  type SetupProgressEvent,
  type DetailedSetupConfig,
} from "../../api/client";

type Step = "mode" | "detailed" | "role" | "environment" | "llm" | "ims_bot_adapter" | "install" | "complete";
type SetupRole = "chat_assistant" | "code_dev_assistant" | "qq_chat_bot" | "ai_butler";


export function useSetupWizard() {
  const router = useRouter();
  const step = ref<Step>("mode");

  const selectedMode = ref<"role" | "detailed" | "skip">("role");
  const selectedRole = ref<SetupRole | null>(null);
  const llmConfig = ref<LlmSetupConfig>({
    mode: "remote",
    model_name: "",
    model_id: null,
    api_endpoint: "",
    api_key: null,
    api_style: "open_ai_chat_completions",
  });
  const imsBotAdapterConfig = ref<ImsBotAdapterSetupConfig>({
    platform: "qq_napcat",
    ws_url: "ws://127.0.0.1:3001",
    qq_id: null,
    token: null,
  });
  const detailedConfig = ref<DetailedSetupConfig>({
    install_method: "docker",
    relational: {
      enabled: true, source: "install", type: "sqlite",
      deployment: { image: "mysql:8.4", port: 3306, data_dir: "./mysql/data", container_name: "zihuan-mysql", restart_policy: "unless-stopped" },
      host: "127.0.0.1", username: "root", password: "", database: "zihuan", sqlite_path: "zihuan_data.db", max_connections: 32, acquire_timeout_secs: 30,
    },
    rustfs: {
      enabled: true, source: "install",
      deployment: { image: "rustfs/rustfs:latest", port: 9000, data_dir: "./rustfs/data", container_name: "zihuan-rustfs", restart_policy: "unless-stopped" },
      endpoint: "http://127.0.0.1:9000", bucket: "zihuan", region: "us-east-1", access_key: "", secret_key: "", public_base_url: null, path_style: true,
    },
    search: {
      enabled: true, source: "install", type: "weaviate",
      deployment: { image: "cr.weaviate.io/semitechnologies/weaviate:1.30.5", port: 8080, data_dir: "./weaviate/data", container_name: "zihuan-weaviate", restart_policy: "unless-stopped" },
      base_url: "http://127.0.0.1:8080", username: null, password: null, api_key: null, vector_dimensions: 1024,
    },
    redis: {
      enabled: false, source: "install",
      deployment: { image: "redis:7", port: 6379, data_dir: "./redis/data", container_name: "zihuan-redis", restart_policy: "unless-stopped" },
      url: "redis://127.0.0.1:6379", username: null, password: null,
    },
  });
  const taskId = ref("");
  const installationMode = ref<"role_based" | "detailed">("role_based");
  const installLogs = ref<SetupProgressEvent[]>([]);
  const installError = ref<string | null>(null);
  let cleanupProgress = (() => {}) as () => void;

  const showProgressBar = computed(() =>
    ["detailed", "environment", "llm", "ims_bot_adapter", "install", "complete"].includes(step.value),
  );

  const progressPercent = computed(() => {
    switch (step.value) {
      case "detailed":
        return 20;
      case "environment":
        return 20;
      case "llm":
        return 40;
      case "ims_bot_adapter":
        return 55;
      case "install":
        return 70;
      case "complete":
        return 100;
      default:
        return 0;
    }
  });

  function onModeSelect(mode: "role" | "detailed" | "skip") {
    selectedMode.value = mode;
    if (mode === "skip") {
      setupApi.skip().then(() => {
        router.push("/");
      });
    } else if (mode === "detailed") {
      step.value = "detailed";
    } else {
      step.value = "role";
    }
  }

  function startDetailedInstallation() {
    startInstallation("detailed");
  }

  function onRoleSelect(role: SetupRole) {
    selectedRole.value = role;
    if (role === "chat_assistant" || role === "code_dev_assistant") {
      step.value = "llm";
    } else {
      step.value = "environment";
    }
  }

  function onLlmNext() {
    if (selectedRole.value === "qq_chat_bot") {
      step.value = "ims_bot_adapter";
    } else {
      startInstallation();
    }
  }

  function onLlmBack() {
    if (selectedRole.value === "chat_assistant" || selectedRole.value === "code_dev_assistant") {
      step.value = "role";
    } else {
      step.value = "environment";
    }
  }

  async function startInstallation(mode: "role_based" | "detailed" = "role_based") {
    installationMode.value = mode;
    step.value = "install";
    installLogs.value = [];
    installError.value = null;
    cleanupProgress();

    try {
      const res = await setupApi.execute(mode === "detailed" ? {
        mode: "detailed",
        detailed_config: detailedConfig.value,
      } : {
        mode: "role_based",
        role: selectedRole.value ?? undefined,
        llm_config: llmConfig.value,
        ims_bot_adapter_config: selectedRole.value === "qq_chat_bot" ? imsBotAdapterConfig.value : undefined,
      });
      taskId.value = res.task_id;

      cleanupProgress = setupApi.streamProgress(res.task_id, (event) => {
        installLogs.value.push(event);
        if (event.status === "error") {
          installError.value = event.error ?? event.message;
        }
        if (event.step === "finished") {
          cleanupProgress();
          setTimeout(() => {
            step.value = "complete";
          }, 500);
        }
      });
    } catch (err) {
      installError.value = String(err);
    }
  }

  function finishSetup() {
    router.push("/");
  }

  return {
    step,
    selectedMode,
    selectedRole,
    llmConfig,
    imsBotAdapterConfig,
    detailedConfig,
    taskId,
    installLogs,
    installError,
    showProgressBar,
    progressPercent,
    onModeSelect,
    onRoleSelect,
    onLlmNext,
    onLlmBack,
    startDetailedInstallation,
    startInstallation,
    finishSetup,
  };
}

export type UseSetupWizardReturn = ReturnType<typeof useSetupWizard>;
