<template>
  <section class="page llm-page">
    <AdminPageHeader title="模型配置">
      <t-button variant="outline" @click="openImportPage">导入配置</t-button>
      <t-button theme="primary" @click="openCreatePage">新建模型</t-button>
    </AdminPageHeader>

    <t-card class="llm-card" bordered>
      <div class="llm-toolbar">
        <t-input v-model="filters.keyword" clearable placeholder="搜索名称、模型名或 Config ID" />
        <t-select v-model="filters.modelType">
          <t-option value="all" label="全部模型类型" />
          <t-option value="chat_llm" label="聊天模型" />
          <t-option value="text_embedding" label="文本向量模型" />
        </t-select>
        <t-select v-model="filters.enabled">
          <t-option value="all" label="全部状态" />
          <t-option value="enabled" label="已启用" />
          <t-option value="disabled" label="已停用" />
        </t-select>
        <div class="llm-toolbar-actions">
          <t-button variant="text" @click="load">刷新</t-button>
          <span class="llm-count">共 {{ filteredItems.length }} 条</span>
        </div>
      </div>

      <t-table row-key="config_id" :data="filteredItems" :columns="columns" :hover="true" :pagination="false" table-layout="fixed">
        <template #name="{ row }">
          <div class="llm-name-cell"><strong>{{ row.name }}</strong><span class="mono">{{ compactId(row.config_id) }}</span></div>
        </template>
        <template #model_type="{ row }"><t-tag variant="light">{{ modelTypeLabel(row.model.type) }}</t-tag></template>
        <template #model="{ row }">
          <div class="llm-model-cell"><span :title="modelName(row)">{{ modelName(row) || "—" }}</span><small v-if="row.model.type === 'chat_llm_api'">{{ apiStyleLabel(row.model.llm.api_style) }}</small><small v-else-if="row.model.type === 'chat_llm_local'">{{ weightStyleLabel(row.model.llm.weight_style) }}</small></div>
        </template>
        <template #endpoint="{ row }">
          <span v-if="row.model.type === 'chat_llm_api'" class="llm-endpoint" :title="row.model.llm.api_endpoint">{{ row.model.llm.api_endpoint || "—" }}</span>
          <span v-else-if="row.model.type === 'text_embedding_api'" class="llm-endpoint" :title="row.model.api_endpoint">{{ row.model.api_endpoint || "—" }}</span>
          <span v-else>本地模型</span>
        </template>
        <template #updated_at="{ row }">{{ formatTime(row.updated_at) }}</template>
        <template #actions="{ row }">
          <div class="llm-actions">
            <t-button variant="text" size="small" @click="router.push('/data-explorer/models/' + encodeURIComponent(row.config_id))">使用详细</t-button>
            <t-button variant="text" size="small" @click="openEditor(row)">编辑</t-button>
            <t-button variant="text" size="small" @click="copyLlmConfig(row)">{{ copiedId === row.config_id ? "已复制" : "复制" }}</t-button>
            <t-popconfirm content="确认删除这个模型配置吗？" @confirm="removeItem(row.config_id)"><t-button variant="text" theme="danger" size="small">删除</t-button></t-popconfirm>
            <t-checkbox
              :checked="row.enabled"
              :disabled="updatingEnabledIds.has(row.config_id)"
              @change="updateEnabled(row, $event)"
            >
              是否启用
            </t-checkbox>
          </div>
        </template>
        <template #empty><div class="llm-empty">暂无匹配的模型配置。</div></template>
      </t-table>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useLlm } from "./useLlm";
import type { LlmConfig } from "../../api/client";

const {
  router,
  filteredItems,
  filters,
  columns,
  load,
  removeItem,
  updateEnabled,
  updatingEnabledIds,
  copiedId,
  copyLlmConfig,
  formatTime,
  compactId,
  modelTypeLabel,
  modelName,
  apiStyleLabel,
  weightStyleLabel,
} = useLlm();

function openCreatePage() {
  void router.push("/llm/edit");
}

function openImportPage() {
  void router.push("/llm/edit?import=1");
}

function openEditor(item: LlmConfig) {
  void router.push({ path: "/llm/edit", query: { id: item.config_id } });
}
</script>

<style scoped lang="scss">
@use "./llm" as *;
</style>
