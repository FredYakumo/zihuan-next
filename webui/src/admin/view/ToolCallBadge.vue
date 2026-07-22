<script setup lang="ts">
import { DeleteIcon, EditIcon, FileIcon } from "tdesign-icons-vue-next";

import { useToolCallBadge, type ToolCallKind } from "../composables/useToolCallBadge";

const props = defineProps<{
  kind: ToolCallKind;
  loading?: boolean;
}>();

const emit = defineEmits<{
  click: [];
}>();

const { kind, loading } = useToolCallBadge(props, emit);
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
      <FileIcon class="badge-icon" />
      {{ kind.filename }}
      <span class="badge-lines badge-lines--added">+{{ kind.lineCount }}行</span>
    </template>
    <template v-else-if="kind.type === 'delete_file'">
      <DeleteIcon class="badge-icon" />
      {{ kind.filename }}
      <span v-if="kind.lineCount != null" class="badge-lines badge-lines--removed">-{{ kind.lineCount }}行</span>
    </template>
    <template v-else-if="kind.type === 'edit_file'">
      <EditIcon class="badge-icon" />
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

<style scoped lang="scss">
@use "../styles/tool-call-badge" as *;
</style>
