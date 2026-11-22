<template>
  <section class="page connections-page">
    <AdminPageHeader title="连接配置">
      <t-button variant="outline" @click="openImportPage">导入配置</t-button>
      <t-button theme="primary" @click="openCreatePage">新建连接</t-button>
    </AdminPageHeader>

    <t-card class="connections-card" bordered>
      <div class="connections-toolbar">
        <t-input v-model="filters.keyword" clearable placeholder="搜索名称、类型或 Config ID" />
        <t-select v-model="filters.type">
          <t-option value="all" label="全部类型" />
          <t-option v-for="type in connectionTypes" :key="type.value" :value="type.value" :label="type.label" />
        </t-select>
        <t-select v-model="filters.enabled">
          <t-option value="all" label="全部状态" />
          <t-option value="enabled" label="已启用" />
          <t-option value="disabled" label="已停用" />
        </t-select>
        <t-button variant="text" @click="load">刷新</t-button>
        <span class="connections-count">共 {{ filteredConnections.length }} 条</span>
      </div>

      <t-table
        row-key="config_id"
        :data="filteredConnections"
        :columns="columns"
        :hover="true"
        :pagination="false"
        table-layout="fixed"
      >
        <template #name="{ row }">
          <div class="connection-name-cell">
            <strong>{{ row.name }}</strong>
            <span class="mono">{{ compactId(row.config_id) }}</span>
          </div>
        </template>
        <template #type="{ row }">
          <t-tag variant="light">{{ connectionTypeLabel(row.kind.type) }}</t-tag>
        </template>
        <template #enabled="{ row }">
          <t-tag variant="light" :theme="row.enabled ? 'success' : 'warning'">
            {{ row.enabled ? "已启用" : "已停用" }}
          </t-tag>
        </template>
        <template #summary="{ row }">
          <span class="connection-summary" :title="connectionSummary(row)">{{ connectionSummary(row) || "—" }}</span>
        </template>
        <template #runtime="{ row }">
          <span>{{ runtimeInstanceCount(row.config_id) }} 个实例</span>
        </template>
        <template #updated_at="{ row }">{{ formatTime(row.updated_at) }}</template>
        <template #actions="{ row }">
          <div class="connection-actions">
            <t-button variant="text" size="small" @click="openEditor(row)">编辑</t-button>
            <t-button variant="text" size="small" @click="openCopy(row)">复制添加</t-button>
            <t-button variant="text" size="small" @click="copyConnectionConfig(row)">
              {{ copiedId === row.config_id ? "已复制" : "复制" }}
            </t-button>
            <t-popconfirm content="确认删除这个连接配置吗？" @confirm="removeConnection(row.config_id)">
              <t-button variant="text" theme="danger" size="small">删除</t-button>
            </t-popconfirm>
          </div>
        </template>
        <template #empty>
          <div class="connections-empty">暂无匹配的连接配置。</div>
        </template>
      </t-table>
    </t-card>

    <ServiceRestartConfirmDialog :state="restartDialog.state" @close="restartDialog.close()" @save-only="restartDialog.saveOnly()" @restart="restartDialog.restartAll()" />
  </section>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ServiceRestartConfirmDialog from "../components/ServiceRestartConfirmDialog.vue";
import { useConnections } from "./useConnections";
import type { ConnectionConfig } from "../../api/client";

const router = useRouter();
const {
  restartDialog,
  filteredConnections,
  filters,
  connectionTypes,
  load,
  removeConnection,
  connectionSummary,
  runtimeInstanceCount,
  formatTime,
  copiedId,
  copyConnectionConfig,
  columns,
  connectionTypeLabel,
  compactId,
} = useConnections();

function openCreatePage() {
  void router.push("/connections/edit");
}

function openImportPage() {
  void router.push("/connections/edit?import=1");
}

function openEditor(connection: ConnectionConfig) {
  void router.push({ path: "/connections/edit", query: { id: connection.config_id } });
}

function openCopy(connection: ConnectionConfig) {
  void router.push({ path: "/connections/edit", query: { copy: connection.config_id } });
}
</script>

<style scoped lang="scss">
@use "./connections" as *;
</style>
