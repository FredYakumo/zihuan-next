<template>
  <section class="page llm-page">
    <AdminPageHeader title="模型配置">
      <t-button variant="outline" @click="triggerImportFile">导入配置</t-button>
      <input ref="importFileInput" type="file" accept=".json" class="llm-import-input" @change="handleFileChange" />
      <t-button theme="primary" @click="startCreate">新建模型</t-button>
    </AdminPageHeader>

    <t-card class="llm-card" bordered>
      <div class="llm-toolbar">
        <t-input v-model="filters.keyword" clearable placeholder="搜索名称、模型名或 Config ID" />
        <t-select v-model="filters.modelType">
          <t-option value="all" label="全部模型类型" />
          <t-option value="chat_llm" label="聊天模型" />
          <t-option value="text_embedding_local" label="文本向量模型" />
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
          <div class="llm-model-cell"><span :title="modelName(row)">{{ modelName(row) || "—" }}</span><small v-if="row.model.type === 'chat_llm'">{{ apiStyleLabel(row.model.llm.api_style) }}</small></div>
        </template>
        <template #endpoint="{ row }">
          <span v-if="row.model.type === 'chat_llm' && !isCandleStyle(row.model.llm.api_style)" class="llm-endpoint" :title="row.model.llm.api_endpoint">{{ row.model.llm.api_endpoint || "—" }}</span>
          <span v-else>本地模型</span>
        </template>
        <template #updated_at="{ row }">{{ formatTime(row.updated_at) }}</template>
        <template #actions="{ row }">
          <div class="llm-actions">
            <t-button variant="text" size="small" @click="editItem(row)">编辑</t-button>
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

    <t-drawer v-model:visible="drawerVisible" :header="isCreating ? '新建模型配置' : '编辑模型配置'" size="680px" :close-on-overlay-click="false" @close="closeDrawer">
      <t-form class="llm-form" label-align="top">
        <div class="llm-form-section">
          <h3>基本信息</h3>
          <div class="llm-form-grid">
            <t-form-item label="名称" required><t-input v-model="form.name" placeholder="例如：OpenAI 主模型" /></t-form-item>
            <t-form-item label="模型类型" required><t-select v-model="form.model_type" :disabled="!isCreating"><t-option value="chat_llm" label="聊天模型" /><t-option value="text_embedding_local" label="文本向量模型" /></t-select></t-form-item>
          </div>
          <t-checkbox v-model="form.enabled">启用该模型配置</t-checkbox>
        </div>

        <template v-if="form.model_type === 'chat_llm'">
          <div class="llm-form-section">
            <h3>模型与接口</h3>
            <div class="llm-form-grid">
              <t-form-item class="llm-form-item--full" label="后端格式"><t-select v-model="form.llm.api_style"><t-option value="candle_gguf" label="Candle GGUF 本地推理" /><t-option value="candle_hf" label="Candle HF 本地推理" /><t-option value="open_ai_chat_completions" label="OpenAI Chat Completions API" /><t-option value="open_ai_chat_completions_tencent_multimodal_compat" label="OpenAI Chat Completions API（腾讯多模态兼容）" /><t-option value="open_ai_responses" label="OpenAI Responses API" /><t-option value="open_ai_responses_message_compat" label="OpenAI Responses API（message 兼容）" /><t-option value="open_ai_responses_image_url_object_compat" label="OpenAI Responses API（image_url 对象兼容）" /></t-select></t-form-item>
              <template v-if="isCandleMode">
                <t-form-item class="llm-form-item--full" label="本地 LLM 目录" required><t-select v-model="form.llm.model_name" placeholder="请选择"><t-option v-for="item in filteredLocalLlmModels" :key="item.model_name" :value="item.model_name" :label="localLlmOptionLabel(item)" :disabled="!item.available" /></t-select><small class="llm-form-hint">{{ selectedLocalLlmHint }}</small></t-form-item>
              </template>
              <template v-else>
                <t-form-item label="Model Name" required><t-input v-model="form.llm.model_name" /></t-form-item>
                <t-form-item label="API Endpoint" required><t-input v-model="form.llm.api_endpoint" /></t-form-item>
                <t-form-item class="llm-form-item--full" label="API Key"><ConnectionCredentialInput v-model="form.llm.api_key" /></t-form-item>
              </template>
            </div>
          </div>

          <div class="llm-form-section">
            <h3>请求参数</h3>
            <div class="llm-form-grid">
              <t-form-item label="Timeout Secs"><t-input-number v-model="form.llm.timeout_secs" :min="1" /></t-form-item>
              <t-form-item label="Retry Count"><t-input-number v-model="form.llm.retry_count" :min="0" /></t-form-item>
              <t-form-item label="上下文长度（tokens）" required>
                <t-input-number v-model="form.llm.context_length" :min="1" :step="1024" :allow-input-over-limit="false" theme="normal" />
                <div class="context-length-presets" aria-label="上下文长度快捷设置">
                  <t-button v-for="preset in contextLengthPresets" :key="preset.label" size="small" variant="outline" @click="form.llm.context_length = preset.value">
                    {{ preset.label }}
                  </t-button>
                </div>
              </t-form-item>
              <t-form-item label="思考模式"><t-select v-model="form.llm.thinking_type"><t-option :value="null" label="未配置" /><t-option value="enabled" label="启用" /><t-option value="disabled" label="关闭" /></t-select></t-form-item>
              <t-form-item label="思考强度"><t-select v-model="form.llm.reasoning_effort"><t-option :value="null" label="未配置" /><t-option value="low" label="低" /><t-option value="medium" label="中" /><t-option value="high" label="高" /><t-option value="max" label="最高" /></t-select></t-form-item>
            </div>
            <div class="llm-switches">
              <t-checkbox v-model="form.llm.supports_multimodal_input" :disabled="isCandleMode">多模态模型（允许传入图片）</t-checkbox>
              <t-checkbox v-model="form.llm.stream">默认启用 stream 请求参数</t-checkbox>
              <t-checkbox v-model="form.llm.include_reasoning_content">推理时回灌 reasoning_content</t-checkbox>
            </div>
          </div>
        </template>

        <div v-else class="llm-form-section">
          <h3>本地向量模型</h3>
          <t-form-item label="本地模型目录" required><t-select v-model="form.local_model_name" placeholder="请选择"><t-option v-for="item in localEmbeddingModels" :key="item" :value="item" :label="item" /></t-select></t-form-item>
        </div>
      </t-form>
      <template #footer><div class="llm-drawer-footer"><t-button variant="outline" @click="closeDrawer">取消</t-button><t-button theme="primary" @click="submitForm">{{ isCreating ? "创建模型" : "保存修改" }}</t-button></div></template>
    </t-drawer>
  </section>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRoute } from "vue-router";

import type { LlmConfig } from "../../api/client";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useLlm } from "../composables/useLlm";
import ConnectionCredentialInput from "./ConnectionCredentialInput.vue";

const { filteredItems, form, drawerVisible, isCreating, filters, localEmbeddingModels, isCandleMode, filteredLocalLlmModels, selectedLocalLlmHint, startCreate, closeDrawer, load, editItem, submitForm, removeItem, updateEnabled, updatingEnabledIds, localLlmOptionLabel, compactId, formatTime, copiedId, copyConfig, handleFileChange } = useLlm();

const route = useRoute();
const importFileInput = ref<HTMLInputElement | null>(null);
const columns = [
  { colKey: "name", title: "配置名称", width: 210 },
  { colKey: "model_type", title: "模型类型", width: 150 },
  { colKey: "model", title: "模型", ellipsis: true },
  { colKey: "endpoint", title: "接口地址", ellipsis: true },
  { colKey: "updated_at", title: "更新时间", width: 170 },
  { colKey: "actions", title: "操作", width: 270, fixed: "right" },
];
const contextLengthPresets = [
  { label: "64K", value: 64 * 1024 },
  { label: "256K", value: 256 * 1024 },
  { label: "1M", value: 1024 * 1024 },
];

function triggerImportFile() { importFileInput.value?.click(); }
if (route.query.action === "create") startCreate();
function modelTypeLabel(type: string) { return type === "chat_llm" ? "聊天模型" : "文本向量模型"; }
function modelName(item: LlmConfig) { return item.model.type === "chat_llm" ? item.model.llm.model_name : item.model.model_name; }
function isCandleStyle(apiStyle: string) { return apiStyle === "candle_gguf" || apiStyle === "candle_hf"; }
function apiStyleLabel(apiStyle: string) {
  const labels: Record<string, string> = { candle_gguf: "Candle GGUF", candle_hf: "Candle HF", open_ai_chat_completions: "OpenAI Chat Completions", open_ai_chat_completions_tencent_multimodal_compat: "腾讯多模态兼容", open_ai_responses: "OpenAI Responses", open_ai_responses_message_compat: "Responses message 兼容", open_ai_responses_image_url_object_compat: "Responses image_url 兼容" };
  return labels[apiStyle] ?? apiStyle;
}
function copyLlmConfig(item: LlmConfig) { copyConfig({ name: item.name, enabled: item.enabled, model: item.model }, item.config_id); }

</script>

<style scoped lang="scss">
@use "../styles/llm" as *;

</style>
