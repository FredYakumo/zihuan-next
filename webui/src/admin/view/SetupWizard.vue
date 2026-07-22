<template>
  <div class="setup-wizard" :class="{ 'setup-wizard--detailed': step === 'detailed' }">
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

      <DetailedConfigStep
        v-else-if="step === 'detailed'"
        v-model="detailedConfig"
        :error="detailedInstallError"
        @next="startDetailedInstallation"
        @back="step = 'mode'"
      />

      <DetailedInstallCommandResult
        v-else-if="step === 'detailed_result' && detailedInstallResult"
        :result="detailedInstallResult"
        @back="step = 'detailed'"
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
        @retry="startInstallation(installationMode)"
      />

      <SetupComplete
        v-else-if="step === 'complete'"
        @finish="finishSetup"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import brandIconSrc from "../../assets/brand-icon.png";
import ModeSelection from "../setup/ModeSelection.vue";
import RoleSelection from "../setup/RoleSelection.vue";
import DetailedConfigStep from "../setup/DetailedConfigStep.vue";
import DetailedInstallCommandResult from "../setup/DetailedInstallCommandResult.vue";
import EnvironmentCheck from "../setup/EnvironmentCheck.vue";
import LlmConfigStep from "../setup/LlmConfigStep.vue";
import ImsBotAdapterConfigStep from "../setup/ImsBotAdapterConfigStep.vue";
import InstallationProgress from "../setup/InstallationProgress.vue";
import SetupComplete from "../setup/SetupComplete.vue";
import { useSetupWizard } from "../composables/useSetupWizard";

const {
  step,
  selectedMode,
  selectedRole,
  llmConfig,
  imsBotAdapterConfig,
  detailedConfig,
  detailedInstallResult,
  detailedInstallError,
  taskId,
  installationMode,
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
} = useSetupWizard();
</script>

<style scoped lang="scss">
@use "../styles/setup-wizard" as *;
</style>
