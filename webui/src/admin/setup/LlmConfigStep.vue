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
          placeholder="例如: GPT-4o, DeepSeek-V4"
        />
      </div>

      <div class="field">
        <label>模型 ID</label>
        <input
          v-model="model.model_id"
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
import type { LlmSetupConfig } from "../../api/client";
import { useLlmConfigStep } from "../composables/useLlmConfigStep";

const model = defineModel<LlmSetupConfig>({ required: true });

defineEmits<{ (e: "next"): void; (e: "back"): void }>();

const { canProceed } = useLlmConfigStep(model);
</script>

<style scoped lang="scss">
@use "../styles/llm-config-step" as *;
</style>
