<template>
  <div class="llm-config-step">
    <h2>配置 LLM</h2>
    <p class="subtitle">设置您的大语言模型</p>

    <div class="tabs">
      <button
        class="tab"
        :class="{ active: model.mode === 'remote' }"
        @click="model.mode = 'remote'"
      >
        远程 API
      </button>
      <button
        class="tab"
        :class="{ active: model.mode === 'local' }"
        @click="model.mode = 'local'"
      >
        本地模型 (Candle)
      </button>
    </div>

    <div v-if="model.mode === 'remote'" class="form-grid">
      <div class="field">
        <label>API 风格</label>
        <select v-model="model.api_style">
          <option value="open_ai_chat_completions">OpenAI Chat Completions</option>
          <option value="open_ai_responses">OpenAI Responses</option>
          <option value="open_ai_chat_completions_tencent_multimodal_compat">
            腾讯多模态兼容
          </option>
        </select>
      </div>

      <div class="field">
        <label>模型名称</label>
        <input
          v-model="model.model_name"
          type="text"
          placeholder="例如: gpt-4o, deepseek-chat"
        />
      </div>

      <div class="field">
        <label>API 地址</label>
        <input
          v-model="model.api_endpoint"
          type="text"
          placeholder="https://api.openai.com/v1/chat/completions"
        />
      </div>

      <div class="field">
        <label>API Key</label>
        <input
          v-model="model.api_key"
          type="password"
          placeholder="sk-..."
        />
      </div>
    </div>

    <div v-else class="form-grid">
      <div class="field">
        <label>模型名称</label>
        <input
          v-model="model.model_name"
          type="text"
          placeholder="例如: llama-2-7b-chat"
        />
      </div>
    </div>

    <div class="actions">
      <button class="btn ghost" @click="$emit('back')">← 返回</button>
      <button
        class="btn primary"
        :disabled="!canProceed"
        @click="$emit('next')"
      >
        下一步 →
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { LlmSetupConfig } from "../../api/client";

const model = defineModel<LlmSetupConfig>({ required: true });

defineEmits<{ (e: "next"): void; (e: "back"): void }>();

const canProceed = computed(() => {
  if (model.value.mode === "remote") {
    return (
      model.value.model_name.trim().length > 0 &&
      model.value.api_endpoint.trim().length > 0
    );
  }
  return model.value.model_name.trim().length > 0;
});
</script>

<style scoped lang="scss">
.llm-config-step {
  text-align: center;

  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    color: var(--admin-ink);
  }

  .subtitle {
    margin: 0 0 24px;
    color: var(--admin-muted);
    font-size: 15px;
  }
}

.tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 24px;
  justify-content: center;
}

.tab {
  padding: 8px 16px;
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  background: var(--admin-bg);
  color: var(--admin-muted);
  cursor: pointer;
  font-size: 14px;

  &.active {
    border-color: var(--admin-accent);
    background: var(--admin-accent-soft);
    color: var(--admin-accent);
    font-weight: 600;
  }
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 16px;
  text-align: left;
  margin-bottom: 24px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;

  label {
    font-size: 13px;
    font-weight: 600;
    color: var(--admin-ink);
  }

  input,
  select {
    padding: 10px 12px;
    border: 1px solid var(--admin-border);
    border-radius: 6px;
    background: var(--admin-bg);
    color: var(--admin-ink);
    font-size: 14px;

    &:focus {
      outline: none;
      border-color: var(--admin-accent);
    }
  }
}

.actions {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
</style>
