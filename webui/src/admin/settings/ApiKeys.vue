<template>
  <section class="page api-keys-page">
    <AdminPageHeader title="API Key" />

    <t-card title="API Keys" bordered header-bordered>
      <template #actions>
        <t-button theme="primary" @click="handleCreateModelHttpApiKey">创建 API Key</t-button>
      </template>
      <t-table :data="modelHttpApiKeys" :columns="columns" row-key="id" :pagination="false" size="small">
        <template #enabled="{ row }">
          <t-switch :value="row.enabled" @change="updateModelHttpApiKey(row, { enabled: $event })" />
        </template>
        <template #actions="{ row }">
          <t-popconfirm content="确认删除此 API Key？" @confirm="deleteModelHttpApiKey(row.id)">
            <t-button variant="text" theme="danger">删除</t-button>
          </t-popconfirm>
        </template>
      </t-table>
    </t-card>

    <t-dialog v-model:visible="secretDialogVisible" header="请立即保存 API Key" :footer="false">
      <p>API Key遗忘后只能重新创建</p>
      <t-input :value="newModelHttpSecret" readonly />
      <div class="api-keys-actions"><t-button theme="primary" @click="copyModelHttpSecret">复制</t-button></div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useApiKeysPage } from "./ApiKeys";

const {
  modelHttpApiKeys,
  newModelHttpSecret,
  updateModelHttpApiKey,
  deleteModelHttpApiKey,
  copyModelHttpSecret,
  secretDialogVisible,
  columns,
  handleCreateModelHttpApiKey,
} = useApiKeysPage();
</script>

<style scoped lang="scss">
@use "./ApiKeys" as *;
</style>
