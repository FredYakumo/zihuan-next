import { computed, ref } from "vue";
import { useRouter } from "vue-router";

import {
  setup as setupApi,
  type ImsBotAdapterSetupConfig,
  type LlmSetupConfig,
  type SetupProgressEvent,
} from "../../api/client";

type Step = "mode" | "role" | "environment" | "llm" | "ims_bot_adapter" | "install" | "complete";
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
  const taskId = ref("");
  const installLogs = ref<SetupProgressEvent[]>([]);
  const installError = ref<string | null>(null);
  let cleanupProgress = (() => {}) as () => void;

  const showProgressBar = computed(() =>
    ["environment", "llm", "ims_bot_adapter", "install", "complete"].includes(step.value),
  );

  const progressPercent = computed(() => {
    switch (step.value) {
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
      setupApi.skip().then(() => {
        router.push("/");
      });
    } else {
      step.value = "role";
    }
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

  async function startInstallation() {
    step.value = "install";
    installLogs.value = [];
    installError.value = null;
    cleanupProgress();

    try {
      const res = await setupApi.execute({
        mode: "role_based",
        role: selectedRole.value ?? undefined,
        llm_config: llmConfig.value,
        ims_bot_adapter_config:
          selectedRole.value === "qq_chat_bot" ? imsBotAdapterConfig.value : undefined,
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
    taskId,
    installLogs,
    installError,
    showProgressBar,
    progressPercent,
    onModeSelect,
    onRoleSelect,
    onLlmNext,
    onLlmBack,
    startInstallation,
    finishSetup,
  };
}

export type UseSetupWizardReturn = ReturnType<typeof useSetupWizard>;
