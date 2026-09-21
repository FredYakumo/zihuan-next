<template>
  <div v-if="confirmation" class="command-approval" :class="{ 'command-approval--input': input }">
    <strong>允许执行此命令？</strong>
    <code class="command-approval__command">{{ confirmation.shell }}&gt; {{ confirmation.command }}</code>
    <div class="command-approval__actions">
      <button class="btn primary" :disabled="pending" @click="$emit('decide', 'once')">执行</button>
      <button class="btn secondary" :disabled="pending" @click="$emit('decide', 'session')">本次对话允许类似指令</button>
      <button class="btn danger" :disabled="pending" @click="$emit('decide', 'reject')">拒绝</button>
    </div>
  </div>
  <div v-if="allowedCommands.length" class="session-command-approvals">
    <strong>本次对话已允许命令</strong>
    <div v-for="family in allowedCommands" :key="family" class="session-command-approval-row">
      <code>{{ family }}</code>
      <button class="session-command-approval-close" :aria-label="`撤回 ${family}`" title="撤回允许" @click="$emit('revoke', family)"><CloseIcon /></button>
    </div>
  </div>
</template>
<script setup lang="ts">
import { CloseIcon } from "tdesign-icons-vue-next";
defineProps<{ confirmation?: { command: string; shell: string } | null; pending?: boolean; allowedCommands: string[]; input?: boolean }>();
defineEmits<{ (event: "decide", decision: "once" | "session" | "reject"): void; (event: "revoke", family: string): void }>();
</script>
<style scoped lang="scss">
@use "./command-approval-panel" as *;
</style>
