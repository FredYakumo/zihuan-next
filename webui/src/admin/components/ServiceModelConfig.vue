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
      header="记忆提示词"
      :confirm-btn="{ content: '保存', loading: memoryPromptsSaving }"
      cancel-btn="取消"
      width="680px"
      top="6vh"
      @confirm="saveMemoryPrompts"
    >
      <div class="memory-prompts-form">
        <p class="memory-prompts-hint">
          记忆代理根据这些提示词决定检索还是写入长期记忆。留空保存会被拒绝；恢复默认可重新打开编辑后取消修改。
        </p>
        <div class="memory-prompts-field">
          <label>系统提示词</label>
          <t-textarea
            v-model="memoryPrompts.system_prompt"
            :autosize="{ minRows: 4, maxRows: 12 }"
            placeholder="记忆代理的系统提示词"
          />
        </div>
        <div class="memory-prompts-field">
          <label>搜索模式提示词（追加到用户消息后）</label>
          <t-textarea
            v-model="memoryPrompts.search_operation_prompt"
            :autosize="{ minRows: 3, maxRows: 10 }"
            placeholder="调用方强制搜索记忆时追加的提示词"
          />
        </div>
        <div class="memory-prompts-field">
          <label>写入模式提示词（追加到用户消息后）</label>
          <t-textarea
            v-model="memoryPrompts.update_operation_prompt"
            :autosize="{ minRows: 3, maxRows: 10 }"
            placeholder="调用方强制写入记忆时追加的提示词"
          />
        </div>
        <p v-if="memoryPromptsError" class="memory-prompts-error">{{ memoryPromptsError }}</p>
      </div>
    </t-dialog>
  </t-card>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { AddIcon, EditIcon } from "tdesign-icons-vue-next";
import {
  getMemoryAgentPromptsSettings,
  request,
  type ConnectionConfig,
  type LlmConfig,
  type MemoryAgentPromptsSettings,
} from "../../api/client";
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

const memoryPromptsVisible = ref(false);
const memoryPromptsSaving = ref(false);
const memoryPromptsError = ref("");
const memoryPrompts = ref<MemoryAgentPromptsSettings>({
  system_prompt: "",
  search_operation_prompt: "",
  update_operation_prompt: "",
});

async function openMemoryPromptsDialog() {
  memoryPromptsError.value = "";
  try {
    memoryPrompts.value = await getMemoryAgentPromptsSettings();
  } catch (cause) {
    memoryPromptsError.value = cause instanceof Error ? cause.message : String(cause);
  }
  memoryPromptsVisible.value = true;
}

async function saveMemoryPrompts() {
  memoryPromptsError.value = "";
  memoryPromptsSaving.value = true;
  try {
    memoryPrompts.value = await request<MemoryAgentPromptsSettings>(
      "PUT",
      "/settings/memory-agent-settings",
      {
        system_prompt: memoryPrompts.value.system_prompt,
        search_operation_prompt: memoryPrompts.value.search_operation_prompt,
        update_operation_prompt: memoryPrompts.value.update_operation_prompt,
      }
    );
    memoryPromptsVisible.value = false;
  } catch (cause) {
    memoryPromptsError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    memoryPromptsSaving.value = false;
  }
}
</script>

<style scoped lang="scss">
@use "./service-model-config" as *;
</style>
