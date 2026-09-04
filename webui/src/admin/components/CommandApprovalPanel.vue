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
<style scoped>
.command-approval { display:flex; align-items:center; flex-wrap:wrap; gap:4px 6px; padding:6px 8px; border:1px solid var(--border,#d9d9d9); border-radius:4px; }
.command-approval--input { order:0; margin:0 20px 8px; background:var(--admin-bg-panel,#fff); }
.command-approval__command { flex-basis:100%; overflow:hidden; padding:4px 6px; border:1px solid #334155; background:#0f172a; color:#f8fafc; font-size:12px; line-height:18px; text-overflow:ellipsis; white-space:nowrap; }
.command-approval__actions { display:flex; flex-wrap:wrap; gap:6px; }
.command-approval .btn { min-height:28px; padding:3px 9px; font-size:13px; }
.session-command-approvals { position:absolute; z-index:1200; right:20px; bottom:calc(100% + 8px); width:min(320px,calc(100vw - 32px)); padding:12px; border:1px solid var(--admin-border,#d9d9d9); border-radius:6px; background:var(--admin-bg-panel,#fff); box-shadow:0 8px 28px rgb(0 0 0 / 18%); }
.session-command-approval-row { display:flex; align-items:center; gap:8px; padding:6px 0; border-top:1px solid var(--admin-border,#eee); }
.session-command-approval-row code { flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.session-command-approval-close { display:inline-grid; flex:0 0 30px; width:30px; height:30px; place-items:center; padding:0; border:0; background:transparent; cursor:pointer; }
</style>
