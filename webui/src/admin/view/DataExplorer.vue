<template>
  <section class="page data-explorer-page">
    <AdminPageHeader title="数据检索" />

    <t-card title="Service" bordered header-bordered>
      <t-table row-key="config_id" :data="explorerServices" :columns="columns" :hover="true">
        <template #name="{ row }">
          <strong>{{ row.name }}</strong>
          <div class="mono service-id">{{ row.config_id }}</div>
        </template>
        <template #type="{ row }"><t-tag variant="light">{{ serviceTypeLabel(row.agent_type.type) }}</t-tag></template>
        <template #status="{ row }"><t-tag variant="light" :theme="row.enabled ? 'success' : 'default'">{{ row.enabled ? row.runtime.status : '已停用' }}</t-tag></template>
        <template #actions="{ row }">
          <t-button v-for="capability in serviceCapabilities(row)" :key="capability" class="action-button" size="small" variant="outline" @click="openDetail(row.config_id, capability)">
            {{ capabilityLabel(capability) }}
          </t-button>
        </template>
      </t-table>
      <div v-if="explorerServices.length === 0" class="empty-state">暂无配置聊天记录、记忆或图片库的 Service。</div>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import type { PrimaryTableCol } from "tdesign-vue-next";
import { useRouter } from "vue-router";

import type { ServiceWithRuntime } from "../../api/client";
import { serviceCapabilities, type ServiceCapability, useDataExplorerList } from "../composables/useDataExplorer";

const router = useRouter();
const { explorerServices } = useDataExplorerList();

const columns: PrimaryTableCol<ServiceWithRuntime>[] = [
  { colKey: "name", title: "Service" },
  { colKey: "type", title: "类型", width: 150 },
  { colKey: "status", title: "状态", width: 120 },
  { colKey: "actions", title: "操作", width: 260 },
];

function serviceTypeLabel(type: string) {
  return type === "qq_chat" ? "QQ Chat" : type === "http_stream" ? "HTTP Stream" : "Workspace";
}

function capabilityLabel(capability: ServiceCapability) {
  return capability === "messages" ? "聊天记录" : capability === "memories" ? "记忆" : "图片库";
}

function openDetail(serviceId: string, capability: ServiceCapability) {
  void router.push(`/data-explorer/${encodeURIComponent(serviceId)}/${capability}`);
}
</script>

<style scoped lang="scss">
.data-explorer-page { display: grid; gap: 16px; }
.service-id { margin-top: 4px; font-size: 12px; color: var(--td-text-color-placeholder); }
.action-button + .action-button { margin-left: 8px; }
.empty-state { padding: 48px 16px; text-align: center; color: var(--td-text-color-placeholder); }
</style>
