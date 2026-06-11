<template>
  <div class="installation-progress">
    <h2>正在配置...</h2>
    <p class="subtitle">请稍候，系统正在自动安装和配置</p>

    <div class="log-console">
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
      <button v-if="error" class="btn primary" @click="$emit('retry')">
        重试
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { SetupProgressEvent } from "../../api/client";

defineProps<{
  taskId: string;
  logs: SetupProgressEvent[];
  error: string | null;
}>();

defineEmits<{ (e: "done"): void; (e: "retry"): void }>();
</script>

<style scoped lang="scss">
.installation-progress {
  text-align: center;

  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    color: var(--admin-ink);
  }

  .subtitle {
    margin: 0 0 20px;
    color: var(--admin-muted);
    font-size: 15px;
  }
}

.log-console {
  text-align: left;
  background: #0d1117;
  border: 1px solid var(--admin-border);
  border-radius: 8px;
  padding: 16px;
  max-height: 320px;
  overflow-y: auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 13px;
  line-height: 1.6;
  margin-bottom: 20px;
}

.log-line {
  display: flex;
  gap: 8px;
  color: #c9d1d9;

  &.success {
    color: #3fb950;
  }

  &.error {
    color: #f85149;
  }

  &.running {
    color: #58a6ff;
  }

  &.skipped {
    color: #8b949e;
  }
}

.log-step {
  color: #8b949e;
  flex-shrink: 0;
}

.log-message {
  flex: 1;
}

.log-pct {
  color: #8b949e;
  flex-shrink: 0;
}

.install-error {
  padding: 12px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--admin-bad) 10%, transparent);
  border: 1px solid var(--admin-bad);
  color: var(--admin-bad);
  font-size: 14px;
  margin-bottom: 20px;
}

.actions {
  display: flex;
  justify-content: center;
  gap: 12px;
}
</style>
