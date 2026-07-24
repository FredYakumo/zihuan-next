<template>
  <div class="detailed-install-command-result">
    <div>
      <h2>安装命令已生成</h2>
      <p class="subtitle">在目标 Linux 机器上执行安装命令，然后将连接配置导入系统配置的 connections 字段。</p>
    </div>

    <section class="command-output">
      <div class="command-output__header">
        <h3>安装命令</h3>
        <button class="btn ghost" @click="copyText(result.install_command, 'command')">
          <CopyIcon /> {{ copied === 'command' ? '已复制' : '复制' }}
        </button>
      </div>
      <textarea readonly :value="result.install_command" aria-label="安装命令" />
    </section>

    <section class="command-output">
      <div class="command-output__header">
        <h3>连接配置 JSON</h3>
        <button class="btn ghost" @click="copyText(connectionsJson, 'connections')">
          <CopyIcon /> {{ copied === 'connections' ? '已复制' : '复制' }}
        </button>
      </div>
      <textarea readonly :value="connectionsJson" aria-label="连接配置 JSON" />
    </section>

    <p v-if="copyError" class="config-error">{{ copyError }}</p>
    <div class="step-actions">
      <button class="btn ghost" @click="$emit('back')"><ArrowLeftIcon /> 返回配置</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { ArrowLeftIcon, CopyIcon } from "tdesign-icons-vue-next";

import type { DetailedInstallCommandResult } from "../../api/client";

const props = defineProps<{ result: DetailedInstallCommandResult }>();
defineEmits<{ (event: "back"): void }>();

const copied = ref<"command" | "connections" | null>(null);
const copyError = ref<string | null>(null);
const connectionsJson = computed(() => JSON.stringify(props.result.connections, null, 2));

async function copyText(value: string, target: "command" | "connections") {
  copyError.value = null;
  try {
    await navigator.clipboard.writeText(value);
    copied.value = target;
    window.setTimeout(() => {
      if (copied.value === target) copied.value = null;
    }, 1600);
  } catch (error) {
    copyError.value = `复制失败：${error instanceof Error ? error.message : String(error)}`;
  }
}
</script>

<style scoped lang="scss">
.detailed-install-command-result { display: grid; gap: 18px; }
h2, h3, p { margin: 0; }
.subtitle { color: var(--admin-muted); }
.command-output { display: grid; gap: 10px; border: 1px solid var(--admin-border); border-radius: 8px; padding: 16px; }
.command-output__header, .step-actions { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
textarea { width: 100%; min-height: 220px; box-sizing: border-box; resize: vertical; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; line-height: 1.5; }
.config-error { color: var(--admin-danger, #c0392b); }
</style>
