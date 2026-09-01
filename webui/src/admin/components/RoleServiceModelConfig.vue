<template>
  <t-card class="agent-service-form-section" :bordered="false">
    <template #title>模型配置</template>
    <div class="agent-service-model-config-grid">
      <t-form-item label="主模型" required>
        <t-select v-model="form.llm_ref_id" placeholder="请选择" @change="emit('primary-model-change', $event)">
          <t-option class="agent-service-add-model-option" value="__add_model__" label="新增模型配置">
            <span class="agent-service-add-model-option-content"><AddIcon />新增模型配置</span>
          </t-option>
          <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
        </t-select>
      </t-form-item>
      <t-form-item class="agent-service-image-understand-item">
        <div class="agent-service-check-row">
          <t-checkbox v-model="form.default_tools_enabled.image_understand">启用视觉理解工具</t-checkbox>
          <t-select
            v-if="form.default_tools_enabled.image_understand"
            v-model="form.image_understand_llm_ref_id"
            placeholder="未选择"
            filterable
            clearable
            @change="emit('image-understand-model-change', $event)"
          >
            <t-option class="agent-service-add-model-option" value="__add_model__" label="新增模型配置">
              <span class="agent-service-add-model-option-content"><AddIcon />新增模型配置</span>
            </t-option>
            <t-option value="" label="未选择" />
            <t-option v-for="item in multimodalChatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
          </t-select>
        </div>
      </t-form-item>
      <t-form-item v-if="form.type === 'workspace'" class="agent-service-agents-item">
        <t-checkbox v-model="form.agents_md_enabled">关注AGENTS.md</t-checkbox>
      </t-form-item>
      <t-form-item
        v-if="form.type === 'workspace'"
        class="agent-service-memory-item"
        :required="form.workspace_memory_enabled"
        :status="form.workspace_memory_enabled && !form.workspace_memory_backend ? 'error' : undefined"
      >
        <div class="agent-service-check-row">
          <t-checkbox v-model="form.workspace_memory_enabled">Agent 记忆</t-checkbox>
          <t-select
            v-if="form.workspace_memory_enabled"
            v-model="form.workspace_memory_backend"
            placeholder="请选择记忆库"
            @change="emit('memory-backend-change', $event)"
          >
            <t-option class="agent-service-add-retrieval-option" value="__add_retrieval_database__" label="新增检索数据库">
              <span class="agent-service-add-model-option-content"><AddIcon />新增检索数据库</span>
            </t-option>
            <t-option value="local_file" label="本地文件" />
            <t-option value="weaviate" label="Weaviate" />
            <t-option value="elasticsearch" label="Elasticsearch" />
          </t-select>
        </div>
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="数学/编程模型">
        <t-select v-model="form.math_programming_llm_ref_id" placeholder="回退主 Brain 模型" clearable>
          <t-option value="" label="回退主 Brain 模型" />
          <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
        </t-select>
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="Preprompt 模型">
        <t-select v-model="form.intent_classification_llm_ref_id" placeholder="回退主 Brain 模型" clearable>
          <t-option value="" label="回退主 Brain 模型" />
          <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
        </t-select>
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="自然语言回复模型">
        <t-select v-model="form.natural_language_reply_llm_ref_id" placeholder="请选择" clearable>
          <t-option value="" label="请选择" />
          <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
        </t-select>
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="分词配置">
        <t-select v-model="form.tokenizer_connection_id" placeholder="不使用（标点分段）" clearable>
          <t-option value="" label="不使用（标点分段）" />
          <t-option v-for="item in tokenizerConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
        </t-select>
      </t-form-item>
    </div>
  </t-card>
</template>

<script setup lang="ts">
import { AddIcon } from "tdesign-icons-vue-next";
import type { ConnectionConfig, LlmConfig } from "../../api/client";
import type { ServiceFormState } from "../model";

defineProps<{
  form: ServiceFormState;
  chatModels: LlmConfig[];
  multimodalChatModels: LlmConfig[];
  tokenizerConnections: ConnectionConfig[];
}>();

const emit = defineEmits<{
  (event: "primary-model-change", value: string | number): void;
  (event: "image-understand-model-change", value: string | number): void;
  (event: "memory-backend-change", value: string | number): void;
}>();
</script>

<style scoped lang="scss">
.agent-service-form-section {
  margin-bottom: 12px;
}

.agent-service-form-section :deep(.t-card__title) {
  font-size: 15px;
  font-weight: 600;
}

.agent-service-model-config-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px 16px;
}

.agent-service-model-config-grid :deep(.t-form__item) {
  display: flex;
  align-items: center;
  margin-bottom: 0;
}

.agent-service-model-config-grid :deep(.t-form__label) {
  flex: 0 0 132px;
  margin: 0 8px 0 0;
  padding: 0;
  white-space: nowrap;
}

.agent-service-model-config-grid :deep(.t-form__controls) {
  flex: 1;
  min-width: 0;
}

.agent-service-model-config-grid :deep(.t-form__controls-content) {
  width: 100%;
}

.agent-service-check-row {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
}

.agent-service-check-row :deep(.t-checkbox) {
  flex-shrink: 0;
  white-space: nowrap;
}

.agent-service-image-understand-item .agent-service-check-row :deep(.t-select) {
  flex: 1;
  min-width: 0;
}

.agent-service-image-understand-item .agent-service-check-row :deep(.t-checkbox),
.agent-service-memory-item .agent-service-check-row :deep(.t-checkbox) {
  flex: 0 0 136px;
}

.agent-service-memory-item .agent-service-check-row :deep(.t-select) {
  flex: 1;
  min-width: 0;
}

@media (max-width: 840px) {
  .agent-service-model-config-grid {
    grid-template-columns: 1fr;
  }
}
</style>
