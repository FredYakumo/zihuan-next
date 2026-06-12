<script setup lang="ts">
type LineEditSpec = {
  start_line: number;
  end_line: number;
  replacement_lines: string[];
};

type ToolCallKind =
  | { type: "create_file"; filename: string; lineCount: number; content: string }
  | { type: "delete_file"; filename: string; lineCount: number | null }
  | { type: "edit_file"; filename: string; addedLines: number; removedLines: number; edits: LineEditSpec[] }
  | { type: "exec_cmd"; command: string; hasResult: boolean; stdout?: string; stderr?: string }
  | { type: "generic"; name: string };

defineProps<{
  kind: ToolCallKind;
  loading?: boolean;
}>();

defineEmits<{
  click: [];
}>();
</script>

<template>
  <span
    class="tool-badge"
    :class="{
      'tool-badge--create': kind.type === 'create_file',
      'tool-badge--delete': kind.type === 'delete_file',
      'tool-badge--edit': kind.type === 'edit_file',
      'tool-badge--cmd': kind.type === 'exec_cmd',
    }"
    @click="$emit('click')"
  >
    <span v-if="loading" class="live-tool-spinner"></span>
    <template v-if="kind.type === 'create_file'">
      <span class="badge-icon">📄</span>
      {{ kind.filename }}
      <span class="badge-lines badge-lines--added">+{{ kind.lineCount }}行</span>
    </template>
    <template v-else-if="kind.type === 'delete_file'">
      <span class="badge-icon">🗑️</span>
      {{ kind.filename }}
      <span v-if="kind.lineCount != null" class="badge-lines badge-lines--removed">-{{ kind.lineCount }}行</span>
    </template>
    <template v-else-if="kind.type === 'edit_file'">
      <span class="badge-icon">✏️</span>
      {{ kind.filename }}
      <span class="badge-lines badge-lines--added">+{{ kind.addedLines }}行</span>
      <span class="badge-lines badge-lines--removed">-{{ kind.removedLines }}行</span>
    </template>
    <template v-else-if="kind.type === 'exec_cmd'">
      <span class="cmd-prefix">&gt;</span>
      {{ kind.command }}
    </template>
  </span>
</template>

<style scoped>
.tool-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 10px;
  border-radius: 6px;
  font-size: 12px;
  line-height: 1.4;
  cursor: pointer;
  border: 1px solid transparent;
  transition: filter 0.15s, box-shadow 0.15s;
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", monospace;
}

.tool-badge:hover {
  filter: brightness(1.15);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18);
}

.tool-badge--create {
  background: color-mix(in srgb, var(--admin-accent) 18%, var(--admin-bg-elevated) 82%);
  border-color: color-mix(in srgb, var(--admin-accent) 35%, transparent);
  color: var(--admin-accent);
}

.tool-badge--delete {
  background: color-mix(in srgb, var(--admin-bad) 16%, var(--admin-bg-elevated) 84%);
  border-color: color-mix(in srgb, var(--admin-bad) 35%, transparent);
  color: var(--admin-bad);
}

.tool-badge--edit {
  background: color-mix(in srgb, var(--admin-good) 16%, var(--admin-bg-elevated) 84%);
  border-color: color-mix(in srgb, var(--admin-good) 35%, transparent);
  color: var(--admin-good);
}

.tool-badge--cmd {
  background: color-mix(in srgb, var(--admin-bg-panel-strong) 80%, var(--admin-ink) 20%);
  border-color: color-mix(in srgb, var(--admin-border-strong) 60%, transparent);
  color: var(--admin-ink);
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", monospace;
  border-radius: 4px;
}

.badge-icon {
  font-size: 13px;
  flex-shrink: 0;
}

.badge-lines {
  opacity: 0.75;
  font-size: 11px;
}

.badge-lines--added {
  color: var(--admin-good);
}

.badge-lines--removed {
  color: var(--admin-bad);
}

.cmd-prefix {
  color: var(--admin-subtle);
  margin-right: 2px;
  user-select: none;
}

.live-tool-spinner {
  display: inline-block;
  width: 10px;
  height: 10px;
  border: 2px solid color-mix(in srgb, var(--admin-accent) 40%, transparent);
  border-top-color: var(--admin-accent);
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
  flex-shrink: 0;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
