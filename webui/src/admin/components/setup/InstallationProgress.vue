<template>
  <div class="installation-progress">
    <h2>{{ title }}</h2>
    <p class="subtitle">{{ subtitle }}</p>

    <div ref="consoleElement" class="log-console" aria-live="polite">
      <div
        v-for="(log, i) in logs"
        :key="i"
        class="log-line"
        :class="log.status"
      >
        <span class="log-step">[{{ log.step }}]</span>
        <span class="log-message">{{ log.message }}</span>
        <span v-if="log.progress_percent != null" class="log-pct">
          {{ log.progress_percent }}%
        </span>
      </div>
      <div v-if="logs.length === 0" class="log-line running">
        <span class="log-message">等待任务启动...</span>
      </div>
    </div>

    <div v-if="error" class="install-error">
      <strong>错误:</strong> {{ error }}
    </div>

    <div class="actions">
      <button class="btn ghost" @click="$emit('back')">{{ backLabel }}</button>
      <button v-if="error" class="btn primary" @click="$emit('retry')">
        重试
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";

import type { SetupProgressEvent } from "../../api/client";
import { useInstallationProgress } from "../../setup/useInstallationProgress";

const props = withDefaults(defineProps<{
  taskId: string;
  logs: SetupProgressEvent[];
  error: string | null;
  title?: string;
  subtitle?: string;
  backLabel?: string;
}>(), {
  title: "正在配置...",
  subtitle: "请稍候，系统正在自动安装和配置",
  backLabel: "返回",
});

defineEmits<{ (e: "done"): void; (e: "retry"): void; (e: "back"): void }>();

const { logs, error } = useInstallationProgress(props);
const consoleElement = ref<HTMLElement | null>(null);

watch(() => logs.value.length, async () => {
  await nextTick();
  if (consoleElement.value) {
    consoleElement.value.scrollTop = consoleElement.value.scrollHeight;
  }
});
</script>

<style scoped lang="scss">
@use "./installation-progress" as *;
</style>
