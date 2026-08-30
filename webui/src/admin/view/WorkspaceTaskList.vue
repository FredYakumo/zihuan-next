<template>
  <div class="workspace-task-list" :class="{ 'workspace-task-list--compact': compact }">
    <div class="workspace-task-panel-title">{{ interrupted ? "TODO（未完成）" : "TODO" }}</div>
    <div
      v-for="task in tasks"
      :key="task.task_id"
      class="workspace-task-item"
      :class="interrupted && task.status !== 'completed' ? 'interrupted' : task.status"
    >
      <CheckCircleIcon v-if="task.status === 'completed'" />
      <LoadingIcon v-else-if="task.status === 'in_progress' && !interrupted" class="workspace-task-loading" />
      <TimeIcon v-else />
      <span>{{ task.subject }}{{ interrupted && task.status !== "completed" ? " · 未完成" : "" }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { CheckCircleIcon, LoadingIcon, TimeIcon } from "tdesign-icons-vue-next";

import type { WorkspaceTask } from "../../api/client";

defineProps<{
  tasks: WorkspaceTask[];
  interrupted?: boolean;
  compact?: boolean;
}>();
</script>
