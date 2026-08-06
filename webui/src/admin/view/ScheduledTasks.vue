<template>
  <section class="page">
    <AdminPageHeader title="计划任务" />
    <t-card bordered header-bordered title="计划任务列表">
      <template #actions><t-button variant="text" @click="load">刷新</t-button></template>
      <div class="scheduled-task-filters">
        <t-select v-model="serviceId" placeholder="选择 QQ Chat Service" @change="load">
          <t-option v-for="service in qqServices" :key="service.config_id" :value="service.config_id" :label="service.name" />
        </t-select>
        <t-select v-model="status" placeholder="全部状态" clearable @change="load">
          <t-option value="pending" label="等待中" /><t-option value="running" label="执行中" />
          <t-option value="succeeded" label="成功" /><t-option value="failed" label="失败" /><t-option value="cancelled" label="已取消" />
        </t-select>
      </div>
      <div v-if="!serviceId" class="empty-state">请选择一个 QQ Chat Service。</div>
      <t-table v-else :data="items" :columns="columns" row-key="id" bordered size="small">
        <template #status="{ row }"><t-tag variant="light">{{ row.status }}</t-tag></template>
        <template #time="{ row }">{{ formatTime(row.start_time) }}<br v-if="row.end_time" />{{ row.end_time ? formatTime(row.end_time) : "" }}</template>
        <template #summary="{ row }"><span class="task-cell-ellipsis">{{ row.info_summary ?? "-" }}</span></template>
        <template #actions="{ row }"><t-button variant="text" theme="danger" size="small" :disabled="row.status !== 'pending'" @click="cancel(row.id)">取消</t-button></template>
      </t-table>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import type { PrimaryTableCol } from "tdesign-vue-next";
import { scheduledTasks, services, type ScheduledTaskEntry } from "../../api/client";

const serviceId = ref("");
const status = ref("");
const items = ref<ScheduledTaskEntry[]>([]);
const serviceItems = ref<Array<{ config_id: string; name: string; agent_type: { type: string } }>>([]);
const qqServices = computed(() => serviceItems.value.filter((service) => service.agent_type.type === "qq_chat"));
function formatTime(value: string) { return new Date(value).toLocaleString(); }
async function load() { if (serviceId.value) items.value = await scheduledTasks.list(serviceId.value, status.value || undefined); }
async function cancel(taskId: string) { await scheduledTasks.cancel(serviceId.value, taskId); await load(); }
onMounted(async () => { serviceItems.value = await services.list() as typeof serviceItems.value; if (qqServices.value.length === 1) { serviceId.value = qqServices.value[0].config_id; await load(); } });
const columns: PrimaryTableCol<ScheduledTaskEntry>[] = [
  { colKey: "task_name", title: "任务名称", width: 120 }, { colKey: "triggered_by", title: "触发者", width: 150 },
  { colKey: "time", title: "开始 / 结束时间", width: 210 }, { colKey: "status", title: "状态", width: 110 },
  { colKey: "summary", title: "信息摘要" }, { colKey: "actions", title: "操作", width: 80 },
];
</script>

<style scoped>
.scheduled-task-filters { display: flex; gap: 12px; margin-bottom: 16px; }
.scheduled-task-filters > * { width: 260px; }
</style>
