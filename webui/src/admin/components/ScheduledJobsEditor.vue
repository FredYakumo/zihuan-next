<template>
  <div>
    <div class="scheduled-jobs-hint">列表中的任务在事件满足时由调度器触发。任务名对应 scheduled_jobs 脚本声明的 task_name。</div>
    <div v-for="(job, index) in jobs" :key="index" style="display: flex; gap: 8px; align-items: center; margin-top: 8px">
      <t-checkbox v-model="job.enabled">启用</t-checkbox>
      <CommonSelect v-model="job.event" :options="EVENT_OPTIONS" style="flex: 2 1 200px" />
      <CommonSelect
        v-model="job.task_name"
        placeholder="选择任务"
        filterable
        :options="taskOptions"
        style="flex: 2 1 140px"
      />
      <t-input-number v-if="job.event === 'sender_silence'" v-model="job.interval_value" :min="1" style="width: 110px" />
      <CommonSelect
        v-if="job.event === 'sender_silence'"
        v-model="job.interval_unit"
        :options="INTERVAL_UNIT_OPTIONS"
        style="width: 80px"
      />
      <t-button variant="text" theme="danger" size="small" @click="emit('remove', index)">删除</t-button>
    </div>
    <div v-if="jobs.length === 0" class="scheduled-jobs-hint" style="margin-top: 6px">尚未配置。点击「添加任务」开始。</div>
    <t-button variant="text" style="margin-top: 6px; padding-left: 0" @click="emit('add')">添加任务</t-button>
    <div v-if="jobs.length > 0 && !rdbConfigured" class="scheduled-jobs-hint">计划任务需要配置关系数据库连接。</div>
  </div>
</template>

<script setup lang="ts">
import { type PropType } from "vue";
import type { QqChatScheduledJobFormItem } from "../model";
import type { SchedulerJobStatus } from "../../api/types";
import CommonSelect from "./CommonSelect.vue";
import { useScheduledJobsEditor } from "./ScheduledJobsEditor";

const props = defineProps({
  jobs: { type: Array as PropType<QqChatScheduledJobFormItem[]>, required: true },
  availableJobs: { type: Array as PropType<SchedulerJobStatus[]>, default: () => [] },
  rdbConfigured: { type: Boolean, default: false },
});

const emit = defineEmits<{
  add: [];
  remove: [index: number];
}>();

const { EVENT_OPTIONS, INTERVAL_UNIT_OPTIONS, taskOptions } = useScheduledJobsEditor(props, emit);
</script>

<style scoped lang="scss">
@use "./ScheduledJobsEditor" as *;
</style>
