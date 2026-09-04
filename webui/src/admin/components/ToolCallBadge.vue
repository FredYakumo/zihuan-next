<script setup lang="ts">
import {
  CodeIcon,
  CopyIcon,
  DeleteIcon,
  EditIcon,
  FileIcon,
  FileSearchIcon,
  FolderSearchIcon,
  GitBranchIcon,
  InfoCircleIcon,
  MoveIcon,
  BookmarkIcon,
  SearchIcon,
  ChatIcon,
  InternetIcon,
} from "tdesign-icons-vue-next";

import { useToolCallBadge, type ToolCallKind } from "./useToolCallBadge";

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
      'tool-badge--find': kind.type === 'find_files',
      'tool-badge--copy': kind.type === 'copy_file',
      'tool-badge--move': kind.type === 'move_file',
      'tool-badge--git': kind.type === 'git_status',
      'tool-badge--memory': kind.type === 'memory_agent',
      'tool-badge--web-search': kind.type === 'web_search',
      'tool-badge--error': kind.type === 'web_search' && kind.error != null,
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
      <CodeIcon class="badge-icon" />
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
    <template v-else-if="kind.type === 'find_files'">
      <FolderSearchIcon class="badge-icon" />
      {{ kind.pattern }}
      <span class="badge-lines">{{ kind.matches.length }}项</span>
    </template>
    <template v-else-if="kind.type === 'git_status'">
      <GitBranchIcon class="badge-icon" />
      {{ kind.branch || 'Git 状态' }}
      <span class="badge-lines">{{ kind.changes.length }}项</span>
    </template>
    <template v-else-if="kind.type === 'copy_file'">
      <CopyIcon class="badge-icon" /> 复制 {{ kind.src }} → {{ kind.dest }}
    </template>
    <template v-else-if="kind.type === 'move_file'">
      <MoveIcon class="badge-icon" /> 移动 {{ kind.src }} → {{ kind.dest }}
    </template>
    <template v-else-if="kind.type === 'file_info'">
      <InfoCircleIcon class="badge-icon" /> {{ kind.filename }}
    </template>
    <template v-else-if="kind.type === 'ask_user'">
      <ChatIcon class="badge-icon" /> {{ kind.question }}
    </template>
    <template v-else-if="kind.type === 'memory_agent'">
      <BookmarkIcon class="badge-icon" />
      {{ kind.action === 'remember' ? '记录记忆' : '回忆记忆' }}
    </template>
    <template v-else-if="kind.type === 'web_search'">
      <InternetIcon class="badge-icon" />
      Web Search
      <span class="badge-lines">{{ kind.url || kind.query || '搜索' }}</span>
    </template>
  </span>
</template>

<style scoped lang="scss">
@use "./tool-call-badge" as *;
</style>
