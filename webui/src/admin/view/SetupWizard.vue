<template>
  <div class="setup-wizard">
    <div class="setup-header">
      <div class="setup-brand">
        <img :src="brandIconSrc" alt="Zihuan Next" class="setup-brand-icon" />
        <h1>Zihuan Next</h1>
        <p>首次设置向导</p>
      </div>
      <div v-if="showProgressBar" class="setup-progress-bar">
        <div
          class="setup-progress-fill"
          :style="{ width: progressPercent + '%' }"
        />
      </div>
    </div>

    <div class="setup-body">
      <ModeSelection
        v-if="step === 'mode'"
        @select="onModeSelect"
      />

      <RoleSelection
        v-else-if="step === 'role'"
        @select="onRoleSelect"
        @back="step = 'mode'"
      />

      <EnvironmentCheck
        v-else-if="step === 'environment'"
        :role="selectedRole"
        @next="step = 'llm'"
        @back="step = 'role'"
      />

      <LlmConfigStep
        v-else-if="step === 'llm'"
        v-model="llmConfig"
        @next="onLlmNext"
        @back="onLlmBack"
      />

      <ImsBotAdapterConfigStep
        v-else-if="step === 'ims_bot_adapter'"
        v-model="imsBotAdapterConfig"
        @next="startInstallation"
        @back="step = 'llm'"
      />

      <InstallationProgress
        v-else-if="step === 'install'"
        :task-id="taskId"
        :logs="installLogs"
        :error="installError"
        @done="step = 'complete'"
        @retry="startInstallation"
      />

      <SetupComplete
        v-else-if="step === 'complete'"
        @finish="finishSetup"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import brandIconSrc from "../../assets/brand-icon.png";
import ModeSelection from "../setup/ModeSelection.vue";
import RoleSelection from "../setup/RoleSelection.vue";
import EnvironmentCheck from "../setup/EnvironmentCheck.vue";
import LlmConfigStep from "../setup/LlmConfigStep.vue";
import ImsBotAdapterConfigStep from "../setup/ImsBotAdapterConfigStep.vue";
import InstallationProgress from "../setup/InstallationProgress.vue";
import SetupComplete from "../setup/SetupComplete.vue";
import { setup as setupApi, type LlmSetupConfig, type ImsBotAdapterSetupConfig, type SetupProgressEvent } from "../../api/client";
import { setLiveLogConsoleVisible } from "../../ui/live_log_console";

type Step = "mode" | "role" | "environment" | "llm" | "ims_bot_adapter" | "install" | "complete";
type SetupRole = "chat_assistant" | "code_dev_assistant" | "qq_chat_bot" | "ai_butler";

const router = useRouter();
const step = ref<Step>("mode");

onMounted(() => {
  setLiveLogConsoleVisible(false);
});

onUnmounted(() => {
  setLiveLogConsoleVisible(true);
});
const selectedMode = ref<"role" | "detailed" | "skip">("role");
const selectedRole = ref<SetupRole | null>(null);
const llmConfig = ref<LlmSetupConfig>({
  mode: "remote",
  model_name: "",
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
  ["environment", "llm", "ims_bot_adapter", "install", "complete"].includes(step.value)
);

const progressPercent = computed(() => {
  switch (step.value) {
    case "environment": return 20;
    case "llm": return 40;
    case "ims_bot_adapter": return 55;
    case "install": return 70;
    case "complete": return 100;
    default: return 0;
  }
});

function onModeSelect(mode: "role" | "detailed" | "skip") {
  selectedMode.value = mode;
  if (mode === "skip") {
    setupApi.skip().then(() => {
      router.push("/");
    });
  } else if (mode === "detailed") {
    // TODO: detailed mode
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
</script>

<style scoped lang="scss">
.setup-wizard {
  min-height: 100vh;
  background: var(--admin-bg);
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 40px 20px;
}

.setup-header {
  width: 100%;
  max-width: 800px;
  margin-bottom: 32px;
}

.setup-brand {
  text-align: center;
  margin-bottom: 24px;

  .setup-brand-icon {
    width: 64px;
    height: 64px;
    margin-bottom: 12px;
  }

  h1 {
    font-size: 28px;
    font-weight: 700;
    color: var(--admin-ink);
    margin: 0 0 4px;
  }

  p {
    font-size: 16px;
    color: var(--admin-muted);
    margin: 0;
  }
}

.setup-progress-bar {
  height: 6px;
  background: var(--admin-border);
  border-radius: 3px;
  overflow: hidden;

  .setup-progress-fill {
    height: 100%;
    background: var(--admin-accent);
    border-radius: 3px;
    transition: width 0.4s ease;
  }
}

.setup-body {
  width: 100%;
  max-width: 800px;
  background: var(--admin-bg-panel);
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  padding: 32px;
}
</style>
