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
import { ref } from "vue";

import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useSettings } from "./useSettings";

const {
  modelHttpApiKeys,
  newModelHttpSecret,
  createModelHttpApiKey,
  updateModelHttpApiKey,
  deleteModelHttpApiKey,
  copyModelHttpSecret,
} = useSettings();

const secretDialogVisible = ref(false);
const columns = [
  { colKey: "name", title: "名称" },
  { colKey: "secret_prefix", title: "Key 前缀" },
  { colKey: "created_at", title: "创建时间" },
  { colKey: "expires_at", title: "过期时间" },
  { colKey: "group", title: "分组" },
  { colKey: "enabled", title: "启用", width: 80 },
  { colKey: "actions", title: "操作", width: 80 },
];

async function handleCreateModelHttpApiKey() {
  await createModelHttpApiKey();
  if (newModelHttpSecret.value) secretDialogVisible.value = true;
}
</script>

<style scoped lang="scss">
.api-keys-actions {
  display: flex;
  margin-top: 16px;
}
</style>
