<template>
  <div class="workspace-task-list" :class="{ 'workspace-task-list--compact': compact }">
    <div class="workspace-task-panel-title">{{ interrupted ? "TODO（未完成）" : "TODO" }}</div>
    <div
      v-for="task in tasks"
      :key="task.task_id"
      class="workspace-task-item"
      :class="interrupted && task.status !== 'completed' ? 'interrupted' : task.status"
      :style="{ color: taskColor(task.status, interrupted) }"
    >
      <span class="workspace-task-icon">
        <CheckCircleIcon v-if="task.status === 'completed'" />
        <LoadingIcon v-else-if="task.status === 'in_progress' && !interrupted" class="workspace-task-loading" />
        <TimeIcon v-else />
      </span>
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

function taskColor(status: WorkspaceTask["status"], interrupted = false): string {
  if (interrupted && status !== "completed") return "#9aa0a6";
  if (status === "completed") return "#2ba471";
  if (status === "pending") return "#eab308";
  return "var(--admin-text-muted)";
}
</script>
