<template>
  <t-card class="agent-service-form-section" :bordered="false">
    <template #title>模型配置</template>
    <div class="agent-service-model-config-grid">
      <t-form-item label="主模型" required>
        <CommonSelect
          v-model="form.llm_ref_id"
          placeholder="请选择"
          :options="chatModelOptions"
          @change="(value) => emit('primary-model-change', value ?? '')"
        >
          <t-option class="agent-service-add-model-option" value="__add_model__" label="新增模型配置">
            <span class="agent-service-add-model-option-content"><AddIcon />新增模型配置</span>
          </t-option>
        </CommonSelect>
      </t-form-item>
      <t-form-item class="agent-service-image-understand-item">
        <div class="agent-service-check-row">
          <t-checkbox v-model="form.default_tools_enabled.image_understand">启用视觉理解工具</t-checkbox>
          <CommonSelect
            v-if="form.default_tools_enabled.image_understand"
            v-model="form.image_understand_llm_ref_id"
            placeholder="未选择"
            filterable
            clearable
            :options="[{ value: '', label: '未选择' }, ...multimodalChatModelOptions]"
            @change="(value) => emit('image-understand-model-change', value ?? '')"
          >
            <t-option class="agent-service-add-model-option" value="__add_model__" label="新增模型配置">
              <span class="agent-service-add-model-option-content"><AddIcon />新增模型配置</span>
            </t-option>
          </CommonSelect>
        </div>
      </t-form-item>
      <t-form-item v-if="form.type === 'workspace'" label="Agent编排模型">
        <CommonSelect
          v-model="form.workspace_orchestration_llm_ref_id"
          placeholder="使用主模型"
          clearable
          filterable
          :options="[{ value: '', label: '使用主模型' }, ...chatModelOptions]"
        />
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
          <CommonSelect
            v-if="form.workspace_memory_enabled"
            v-model="form.workspace_memory_backend"
            placeholder="请选择记忆库"
            :options="[
              { value: 'local_file', label: '本地文件' },
              { value: 'retrieval_store', label: '检索数据库' },
            ]"
            @change="(value) => emit('memory-backend-change', value ?? '')"
          >
            <t-option class="agent-service-add-retrieval-option" value="__add_retrieval_database__" label="新增检索数据库">
              <span class="agent-service-add-model-option-content"><AddIcon />新增检索数据库</span>
            </t-option>
          </CommonSelect>
        </div>
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="数学/编程模型">
        <CommonSelect
          v-model="form.math_programming_llm_ref_id"
          placeholder="回退主 Brain 模型"
          clearable
          :options="[{ value: '', label: '回退主 Brain 模型' }, ...chatModelOptions]"
        />
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="Preprompt 模型">
        <CommonSelect
          v-model="form.intent_classification_llm_ref_id"
          placeholder="回退主 Brain 模型"
          clearable
          :options="[{ value: '', label: '回退主 Brain 模型' }, ...chatModelOptions]"
        />
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="自然语言回复模型">
        <CommonSelect
          v-model="form.natural_language_reply_llm_ref_id"
          placeholder="请选择"
          clearable
          :options="[{ value: '', label: '请选择' }, ...chatModelOptions]"
        />
      </t-form-item>
      <t-form-item v-if="form.type === 'qq_chat'" label="分词配置">
        <CommonSelect
          v-model="form.tokenizer_connection_id"
          placeholder="不使用（标点分段）"
          clearable
          :options="[{ value: '', label: '不使用（标点分段）' }, ...tokenizerOptions]"
        />
      </t-form-item>
    </div>
  </t-card>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { AddIcon } from "tdesign-icons-vue-next";
import type { ConnectionConfig, LlmConfig } from "../../api/client";
import type { ServiceFormState } from "../model";
import CommonSelect from "./CommonSelect.vue";

const props = defineProps<{
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

const chatModelOptions = computed(() =>
  props.chatModels.map((model) => ({ value: model.config_id, label: model.name })),
);
const multimodalChatModelOptions = computed(() =>
  props.multimodalChatModels.map((model) => ({ value: model.config_id, label: model.name })),
);
const tokenizerOptions = computed(() =>
  props.tokenizerConnections.map((connection) => ({
    value: connection.config_id,
    label: connection.name,
  })),
);
</script>

<style scoped lang="scss">
@use "./service-model-config" as *;
</style>
