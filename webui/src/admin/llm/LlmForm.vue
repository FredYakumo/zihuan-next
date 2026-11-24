<template>
  <t-form class="llm-form" label-align="top">
    <div class="llm-form-section">
      <h3>基本信息</h3>
      <div class="llm-form-grid">
        <t-form-item label="名称" required><t-input v-model="form.name" placeholder="例如：OpenAI 主模型" /></t-form-item>
        <t-form-item label="模型类型" required><t-select v-model="modelTypeSelect" :disabled="!isCreating"><t-option value="chat_llm" label="聊天模型" /><t-option value="text_embedding" label="文本向量模型" /></t-select></t-form-item>
      </div>
      <t-checkbox v-model="form.enabled">启用该模型配置</t-checkbox>
    </div>

    <template v-if="isChatMode">
      <div class="llm-form-section">
        <h3>模型与接口</h3>
        <div class="llm-form-grid">
          <t-form-item class="llm-form-item--full" label="接入方式">
            <t-radio-group v-model="accessMode" variant="default-filled">
              <t-radio-button value="local">本地模型</t-radio-button>
              <t-radio-button value="api">API</t-radio-button>
            </t-radio-group>
          </t-form-item>
          <template v-if="form.model_type === 'chat_llm_local'">
            <t-form-item label="运行方式" required><t-select v-model="form.llm.weight_style"><t-option value="gguf" label="GGUF 本地推理" /><t-option value="hf" label="HF safetensors 本地推理" /><t-option value="mock" label="本地模拟（录制回放）" /></t-select></t-form-item>
            <template v-if="form.llm.weight_style === 'mock'">
              <t-form-item label="Model Name" required><t-input v-model="form.llm.model_name" /></t-form-item>
              <t-form-item class="llm-form-item--full" label="录制目录" required><t-input v-model="form.llm.api_endpoint" placeholder="调用记录所在目录" /></t-form-item>
            </template>
            <template v-else>
              <t-form-item class="llm-form-item--full" label="本地 LLM 目录" required><t-select v-model="form.llm.model_name" placeholder="请选择"><t-option v-for="item in filteredLocalLlmModels" :key="item.model_name" :value="item.model_name" :label="localLlmOptionLabel(item)" :disabled="!item.available" /></t-select><small class="llm-form-hint">{{ selectedLocalLlmHint }}</small></t-form-item>
            </template>
          </template>
          <template v-else>
            <t-form-item class="llm-form-item--full" label="后端格式"><t-select v-model="form.llm.api_style"><t-option value="open_ai_chat_completions" label="OpenAI Chat Completions API" /><t-option value="open_ai_chat_completions_tencent_multimodal_compat" label="OpenAI Chat Completions API（腾讯多模态兼容）" /><t-option value="open_ai_responses" label="OpenAI Responses API" /><t-option value="open_ai_responses_message_compat" label="OpenAI Responses API（message 兼容）" /><t-option value="open_ai_responses_image_url_object_compat" label="OpenAI Responses API（image_url 对象兼容）" /></t-select></t-form-item>
            <t-form-item label="Model Name" required><t-input v-model="form.llm.model_name" /></t-form-item>
            <t-form-item label="API Endpoint" required><t-input v-model="form.llm.api_endpoint" /></t-form-item>
            <t-form-item class="llm-form-item--full" label="API Key"><ConnectionCredentialInput v-model="form.llm.api_key" /></t-form-item>
            <t-form-item class="llm-form-item--full" label="使用代理">
              <t-radio-group v-model="useProxy" variant="default-filled">
                <t-radio-button value="no">不使用代理</t-radio-button>
                <t-radio-button value="yes">使用代理</t-radio-button>
              </t-radio-group>
            </t-form-item>
            <template v-if="useProxy === 'yes'">
              <t-form-item class="llm-form-item--full" label="代理类型">
                <t-radio-group v-model="form.llm.proxy!.kind" variant="default-filled">
                  <t-radio-button value="http">HTTP</t-radio-button>
                  <t-radio-button value="socks5">SOCKS5</t-radio-button>
                </t-radio-group>
              </t-form-item>
              <t-form-item label="代理地址" required><t-input v-model="form.llm.proxy!.host" placeholder="例如：127.0.0.1" /></t-form-item>
              <t-form-item label="端口" required><t-input-number v-model="form.llm.proxy!.port" :min="1" :max="65535" /></t-form-item>
              <t-form-item label="代理用户名"><t-input v-model="form.llm.proxy!.username" placeholder="选填" /></t-form-item>
              <t-form-item label="代理密码"><ConnectionCredentialInput v-model="form.llm.proxy!.password" placeholder="选填" /></t-form-item>
            </template>
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
          <div class="llm-form-grid llm-form-grid--triple llm-form-item--full">
            <t-form-item label="思考字段风格">
              <t-select v-model="form.llm.reasoning_param_style">
                <t-option value="thinking" label="thinking 字段(DeepSeek/Qwen/Tencent)" />
                <t-option value="reasoning_enabled" label="reasoning.enabled(OpenAI兼容)" />
              </t-select>
            </t-form-item>
            <t-form-item label="思考模式"><t-select v-model="form.llm.thinking_type"><t-option :value="null" label="未配置" /><t-option value="enabled" label="启用" /><t-option value="disabled" label="关闭" /></t-select></t-form-item>
            <t-form-item label="思考强度"><t-select v-model="form.llm.reasoning_effort"><t-option :value="null" label="未配置" /><t-option value="low" label="低" /><t-option value="medium" label="中" /><t-option value="high" label="高" /><t-option value="max" label="最高" /></t-select></t-form-item>
          </div>
        </div>
        <div class="llm-switches">
          <t-checkbox v-model="form.llm.supports_multimodal_input" :disabled="form.model_type === 'chat_llm_local'">多模态模型（允许传入图片）</t-checkbox>
          <t-checkbox v-model="form.llm.stream">默认启用 stream 请求参数</t-checkbox>
          <t-checkbox v-model="form.llm.include_reasoning_content">推理时回灌 reasoning_content</t-checkbox>
        </div>
      </div>
    </template>

    <div v-else class="llm-form-section">
      <h3>向量模型与接口</h3>
      <div class="llm-form-grid">
        <t-form-item class="llm-form-item--full" label="接入方式">
          <t-radio-group v-model="accessMode" variant="default-filled">
            <t-radio-button value="local">本地模型</t-radio-button>
            <t-radio-button value="api">API</t-radio-button>
          </t-radio-group>
        </t-form-item>
        <template v-if="form.model_type === 'text_embedding_api'">
          <t-form-item label="Model Name" required><t-input v-model="form.embedding.model_name" /></t-form-item>
          <t-form-item label="API Endpoint" required><t-input v-model="form.embedding.api_endpoint" placeholder="例如：https://api.openai.com/v1/embeddings" /></t-form-item>
          <t-form-item class="llm-form-item--full" label="API Key"><ConnectionCredentialInput v-model="form.embedding.api_key" /></t-form-item>
          <t-form-item class="llm-form-item--full" label="使用代理">
            <t-radio-group v-model="useEmbeddingProxy" variant="default-filled">
              <t-radio-button value="no">不使用代理</t-radio-button>
              <t-radio-button value="yes">使用代理</t-radio-button>
            </t-radio-group>
          </t-form-item>
          <template v-if="useEmbeddingProxy === 'yes'">
            <t-form-item class="llm-form-item--full" label="代理类型">
              <t-radio-group v-model="form.embedding.proxy!.kind" variant="default-filled">
                <t-radio-button value="http">HTTP</t-radio-button>
                <t-radio-button value="socks5">SOCKS5</t-radio-button>
              </t-radio-group>
            </t-form-item>
            <t-form-item label="代理地址" required><t-input v-model="form.embedding.proxy!.host" placeholder="例如：127.0.0.1" /></t-form-item>
            <t-form-item label="端口" required><t-input-number v-model="form.embedding.proxy!.port" :min="1" :max="65535" /></t-form-item>
            <t-form-item label="代理用户名"><t-input v-model="form.embedding.proxy!.username" placeholder="选填" /></t-form-item>
            <t-form-item label="代理密码"><ConnectionCredentialInput v-model="form.embedding.proxy!.password" placeholder="选填" /></t-form-item>
          </template>
          <t-form-item label="Timeout Secs"><t-input-number v-model="form.embedding.timeout_secs" :min="1" /></t-form-item>
          <t-form-item label="Retry Count"><t-input-number v-model="form.embedding.retry_count" :min="0" /></t-form-item>
        </template>
        <template v-else>
          <t-form-item class="llm-form-item--full" label="本地模型目录" required><t-select v-model="form.local_model_name" placeholder="请选择"><t-option v-for="item in localEmbeddingModels" :key="item" :value="item" :label="item" /></t-select></t-form-item>
        </template>
      </div>
      <t-button v-if="form.model_type === 'text_embedding_local'" variant="outline" @click="downloadDialogVisible = true">下载模型…</t-button>
    </div>
  </t-form>
</template>

<script lang="ts">
import { defineComponent, type PropType } from "vue";
import ConnectionCredentialInput from "../components/ConnectionCredentialInput.vue";
import type { UseLlmReturn } from "./useLlm";

export default defineComponent({
  name: "LlmForm",
  components: { ConnectionCredentialInput },
  props: {
    page: { type: Object as PropType<UseLlmReturn>, required: true },
  },
  setup(props) {
    return { ...props.page };
  },
});
</script>

<style scoped lang="scss">
@use "./llm" as *;
</style>
