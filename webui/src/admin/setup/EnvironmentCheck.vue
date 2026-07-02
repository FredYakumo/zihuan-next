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
import { useEnvironmentCheck } from "../composables/useEnvironmentCheck";

defineProps<{
  role: string | null;
}>();

defineEmits<{ (e: "next"): void; (e: "back"): void }>();

const { loading, info, compilerLabel } = useEnvironmentCheck();
</script>

<style scoped lang="scss">
@use "../styles/environment-check" as *;
</style>
