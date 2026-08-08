<script setup lang="ts">
import {
  CodeIcon,
  DeleteIcon,
  EditIcon,
  FileIcon,
  FileSearchIcon,
  FolderSearchIcon,
  SearchIcon,
} from "tdesign-icons-vue-next";

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
      'tool-badge--read': kind.type === 'read_file',
      'tool-badge--list': kind.type === 'list_dir',
      'tool-badge--grep': kind.type === 'grep',
      'tool-badge--rg': kind.type === 'rg',
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
    <template v-else-if="kind.type === 'read_file'">
      <FileSearchIcon class="badge-icon" />
      {{ kind.filename }}
      <span v-if="kind.startLine != null && kind.endLine != null" class="badge-lines">
        L{{ kind.startLine }}-{{ kind.endLine }}
      </span>
    </template>
    <template v-else-if="kind.type === 'list_dir'">
      <FolderSearchIcon class="badge-icon" />
      {{ kind.dirname }}
      <span class="badge-lines">{{ kind.entries.length }}项</span>
    </template>
    <template v-else-if="kind.type === 'grep'">
      <SearchIcon class="badge-icon" />
      {{ kind.pattern }}
      <span class="badge-lines">{{ kind.totalMatches }}处</span>
    </template>
    <template v-else-if="kind.type === 'rg'">
      <CodeIcon class="badge-icon" />
      {{ kind.pattern }}
      <span class="badge-lines">{{ kind.totalMatches }}处</span>
    </template>
  </span>
</template>

<style scoped lang="scss">
@use "../styles/tool-call-badge" as *;
</style>
