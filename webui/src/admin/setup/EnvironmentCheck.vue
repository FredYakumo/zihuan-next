<template>
  <div class="environment-check">
    <h2>系统环境监测</h2>
    <p class="subtitle">检测系统环境和可用服务...</p>

    <div v-if="loading" class="env-loading">
      <div class="spinner"></div>
      <span>检测中...</span>
    </div>

    <div v-else class="env-grid">
      <!-- OS: full width -->
      <div class="env-item full-width">
        <span class="env-status ok">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
        </span>
        <span class="env-label">Operating System:</span>
        <span class="env-value">{{ info.os_detail || info.os }}</span>
      </div>

      <!-- Docker / Docker Compose: two columns -->
      <div class="env-item" :class="{ ok: info.docker_available, fail: !info.docker_available }">
        <span class="env-status">
          <svg v-if="info.docker_available" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </span>
        <span class="env-label">Docker Service:</span>
        <span class="env-value">{{ info.docker_available ? 'Installed' : 'Not installed' }}</span>
      </div>

      <div class="env-item" :class="{ ok: info.docker_compose_available, fail: !info.docker_compose_available }">
        <span class="env-status">
          <svg v-if="info.docker_compose_available" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </span>
        <span class="env-label">Docker Compose support:</span>
        <span class="env-value">{{ info.docker_compose_available ? 'Available' : 'Unavailable' }}</span>
      </div>

      <!-- CUDA / Compiler: two columns -->
      <div class="env-item" :class="{ ok: !!info.cuda_version, fail: !info.cuda_version }">
        <span class="env-status">
          <svg v-if="info.cuda_version" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/></svg>
        </span>
        <span class="env-label">CUDA Environment:</span>
        <span class="env-value">{{ info.cuda_version ?? 'Not detected' }}</span>
      </div>

      <div class="env-item" :class="{ ok: !!info.compiler_version, fail: !info.compiler_version }">
        <span class="env-status">
          <svg v-if="info.compiler_version" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/></svg>
        </span>
        <span class="env-label">{{ compilerLabel }}:</span>
        <span class="env-value">{{ info.compiler_version ?? 'Not detected' }}</span>
      </div>

      <!-- Proxy: full width -->
      <div class="env-item full-width" :class="{ ok: !!info.proxy }">
        <span class="env-status">
          <svg v-if="info.proxy" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/></svg>
        </span>
        <span class="env-label">System Proxy:</span>
        <span class="env-value">{{ info.proxy ? info.proxy.replace('http://', '') : 'No proxy detected (scanned ports 7890, 1080, 7897, 10808, 8080)' }}</span>
      </div>
    </div>

    <div class="env-actions">
      <button class="btn ghost" @click="$emit('back')">← Back</button>
      <button class="btn primary" @click="$emit('next')">Next →</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { setup as setupApi, type EnvironmentInfo } from "../../api/client";

const props = defineProps<{
  role: string | null;
}>();

defineEmits<{ (e: "next"): void; (e: "back"): void }>();

const loading = ref(true);
const info = ref<EnvironmentInfo>({
  os: "",
  os_detail: "",
  docker_available: false,
  docker_compose_available: false,
  cuda_version: null,
  compiler_version: null,
  proxy: null,
  services: [],
});

const compilerLabel = computed(() => {
  const os = info.value.os.toLowerCase();
  if (os === "windows") return "MSVC/GCC/LLVM";
  if (os === "macos") return "Clang/LLVM";
  return "GCC/LLVM";
});

onMounted(async () => {
  try {
    info.value = await setupApi.getEnvironment();
  } catch (err) {
    console.error("Failed to detect environment", err);
  } finally {
    loading.value = false;
  }
});

</script>

<style scoped lang="scss">
.environment-check {
  text-align: center;

  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    color: var(--admin-ink);
  }

  .subtitle {
    margin: 0 0 24px;
    color: var(--admin-muted);
    font-size: 15px;
  }
}

.env-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 40px;
  color: var(--admin-muted);
}

.spinner {
  width: 24px;
  height: 24px;
  border: 2px solid var(--admin-border);
  border-top-color: var(--admin-accent);
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.env-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
  margin-bottom: 20px;

  @media (max-width: 480px) {
    grid-template-columns: 1fr;
  }
}

.env-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px;
  border: 1px solid var(--admin-border);
  border-radius: 8px;
  background: var(--admin-bg);

  .env-status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    color: var(--admin-muted);
  }

  .env-label {
    font-weight: 600;
    color: var(--admin-ink);
  }

  .env-value {
    margin-left: auto;
    font-size: 13px;
    color: var(--admin-muted);
  }

  &.ok {
    border-color: var(--admin-good);
    background: color-mix(in srgb, var(--admin-good) 8%, transparent);

    .env-status {
      color: var(--admin-good);
    }
  }

  &.fail {
    border-color: var(--admin-bad);
    background: color-mix(in srgb, var(--admin-bad) 8%, transparent);

    .env-status {
      color: var(--admin-bad);
    }
  }

  &.full-width {
    grid-column: span 2;

    @media (max-width: 480px) {
      grid-column: span 1;
    }
  }
}

.env-notice {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--admin-accent) 10%, transparent);
  border: 1px solid var(--admin-accent);
  color: var(--admin-accent);
  font-size: 13px;
  margin-bottom: 12px;
}

.env-warning {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--admin-warn) 10%, transparent);
  border: 1px solid var(--admin-warn);
  color: var(--admin-warn);
  font-size: 13px;
  margin-bottom: 20px;
}

.env-actions {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
</style>
