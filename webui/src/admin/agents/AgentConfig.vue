<template>
  <section class="page agent-config-page">
    <AdminPageHeader title="Agent 配置">
      <t-button theme="primary" @click="openCreatePage">新增 Agent</t-button>
    </AdminPageHeader>
    <t-card bordered>
      <t-table :data="subagents" :columns="columns" row-key="id" :loading="loading">
        <template #name="{ row }"><div class="agent-name-cell"><span>{{ row.name }}</span><t-tag size="small" variant="light" theme="primary">SubAgent</t-tag><t-tag v-if="row.builtin" size="small" variant="light" theme="success">Built-in</t-tag></div></template>
        <template #tools="{ row }">{{ row.tool_ids.length }} 个工具</template>
        <template #actions="{ row }"><t-button variant="text" size="small" @click="openEditorPage(row.id)">编辑</t-button><t-popconfirm content="确认删除这个 SubAgent 吗？默认 Agent 将在下次启动时自动恢复。" @confirm="remove(row.id)"><t-button variant="text" theme="danger" size="small">删除</t-button></t-popconfirm></template>
        <template #empty><div class="agent-config-empty">{{ listError ? "SubAgent 列表加载失败。" : "暂无 SubAgent。" }}</div></template>
      </t-table>
    </t-card>
    <t-alert v-if="listError" theme="error" :message="listError" class="agent-config-alert" />

    <ServiceRestartConfirmDialog :state="restartDialog.state" @close="restartDialog.close()" @save-only="restartDialog.saveOnly()" @restart="restartDialog.restartAll()" />
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ServiceRestartConfirmDialog from "../components/ServiceRestartConfirmDialog.vue";
import { useAgentConfig } from "./AgentConfig";

const {
  router,
  restartDialog,
  subagents,
  loading,
  listError,
  columns,
  remove,
} = useAgentConfig();

function openCreatePage() {
  void router.push("/agent-config/edit");
}

function openEditorPage(id: string) {
  void router.push({ path: "/agent-config/edit", query: { id } });
}
</script>

<style scoped lang="scss">
@use "./AgentConfig" as *;
</style>
