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
      <!-- label-width 0 keeps TDesign from rendering an empty label placeholder that would indent these label-less rows. -->
      <t-form-item :label-width="0" class="agent-service-image-understand-item">
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
      <t-form-item v-if="form.type === 'workspace'" :label-width="0" class="agent-service-agents-item">
        <t-checkbox v-model="form.agents_md_enabled">关注AGENTS.md</t-checkbox>
      </t-form-item>
      <t-form-item
        v-if="form.type === 'workspace'"
        class="agent-service-memory-item"
        :label-width="0"
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
          <t-button
            v-if="form.workspace_memory_enabled"
            variant="text"
            shape="square"
            title="编辑记忆提示词"
            @click="openMemoryPromptsDialog"
          >
            <EditIcon />
          </t-button>
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
    <t-dialog
      v-model:visible="memoryPromptsVisible"
      header="Memory Agent配置"
      :confirm-btn="{ content: '保存', loading: memoryPromptsSaving }"
      cancel-btn="取消"
      width="680px"
      top="6vh"
      @confirm="saveMemoryPrompts"
    >
      <div class="memory-prompts-form">
        <div class="memory-prompts-field">
          <label>Agent System Prompt</label>
          <t-textarea
            v-model="memoryPrompts.system_prompt"
            :autosize="{ minRows: 4, maxRows: 12 }"
            :placeholder="memoryPromptDefaults.system_prompt"
          />
        </div>
        <div class="memory-prompts-field">
          <label>Search memory prompt</label>
          <t-textarea
            v-model="memoryPrompts.search_operation_prompt"
            :autosize="{ minRows: 3, maxRows: 10 }"
            :placeholder="memoryPromptDefaults.search_operation_prompt"
          />
        </div>
        <div class="memory-prompts-field">
          <label>Write memory</label>
          <t-textarea
            v-model="memoryPrompts.update_operation_prompt"
            :autosize="{ minRows: 3, maxRows: 10 }"
            :placeholder="memoryPromptDefaults.update_operation_prompt"
          />
        </div>
        <p v-if="memoryPromptsError" class="memory-prompts-error">{{ memoryPromptsError }}</p>
      </div>
    </t-dialog>
  </t-card>
</template>

<script setup lang="ts">
import { AddIcon, EditIcon } from "tdesign-icons-vue-next";
import type { ConnectionConfig, LlmConfig } from "../../api/client";
import type { ServiceFormState } from "../model";
import CommonSelect from "./CommonSelect.vue";
import { useServiceModelConfig } from "./ServiceModelConfig";

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

const {
  chatModelOptions,
  multimodalChatModelOptions,
  tokenizerOptions,
  memoryPromptsVisible,
  memoryPromptsSaving,
  memoryPromptsError,
  memoryPrompts,
  memoryPromptDefaults,
  openMemoryPromptsDialog,
  saveMemoryPrompts,
} = useServiceModelConfig(props, emit);
</script>

<style scoped lang="scss">
@use "./service-model-config" as *;
</style>
