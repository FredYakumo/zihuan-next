<template>
  <section class="page">
    <div class="page-hero">
      <h2>模型配置</h2>
      <div class="hero-actions connection-hero-actions">
        <button class="btn ghost" @click="triggerImportFile">导入</button>
        <input ref="importFileInput" type="file" accept=".json" hidden @change="handleFileChange" />
        <button class="btn primary connection-hero-add-btn" @click="startCreate">+</button>
      </div>
    </div>

    <section class="panel">
      <div class="toolbar">
        <input v-model="filters.keyword" placeholder="搜索名称、配置 ID 或模型" />
        <select v-model="filters.modelType">
          <option value="all">全部类型</option>
          <option value="chat_llm">聊天模型</option>
          <option value="text_embedding_local">本地向量模型</option>
        </select>
        <select v-model="filters.enabled">
          <option value="all">全部状态</option>
          <option value="enabled">已启用</option>
          <option value="disabled">已停用</option>
        </select>
      </div>
      <div v-if="filteredItems.length" class="connection-grid">
        <article v-for="item in filteredItems" :key="item.config_id" class="connection-card">
          <div class="connection-card-header connection-card-header--stacked">
            <div class="connection-card-header-top">
              <div class="connection-card-badges">
                <span class="badge">model</span>
                <span class="badge" :class="item.enabled ? 'success' : ''">{{ item.enabled ? "已启用" : "已停用" }}</span>
              </div>
              <div class="inline-actions">
                <button class="btn ghost connection-card-compact-btn" @click="editItem(item)">编辑</button>
                <button class="btn ghost connection-card-compact-btn" @click="copyLlmConfig(item)">{{ copiedId === item.config_id ? "已复制" : "复制" }}</button>
                <button class="btn warn connection-card-compact-btn" @click="removeItem(item.config_id)">删除</button>
              </div>
            </div>
            <h4>{{ item.name }}</h4>
          </div>
          <div class="connection-card-body">
            <div class="key-value"><strong>Config ID</strong><span class="mono">{{ compactId(item.config_id) }}</span></div>
            <div class="key-value"><strong>类型</strong><span>{{ item.model.type }}</span></div>
            <div class="key-value"><strong>模型</strong><span>{{ item.model.type === "chat_llm" ? item.model.llm.model_name : item.model.model_name }}</span></div>
            <div v-if="item.model.type === 'chat_llm'" class="key-value"><strong>后端格式</strong><span>{{ item.model.llm.api_style }}</span></div>
          </div>
        </article>
      </div>
      <p v-else class="muted">暂无匹配的模型配置。</p>
    </section>

    <div v-if="drawerVisible" class="connection-picker-backdrop">
      <div class="connection-picker-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>{{ isCreating ? "新建模型配置" : "编辑模型配置" }}</h3>
          <button class="btn ghost connection-card-compact-btn" @click="closeDrawer">关闭</button>
        </div>
        <div class="connection-picker-form">
          <div class="form-grid">
            <div class="field"><label>名称</label><input v-model="form.name" /></div>
            <div class="field"><label>类型</label><select v-model="form.model_type"><option value="chat_llm">聊天模型</option><option value="text_embedding_local">本地向量模型</option></select></div>
            <div class="field-full field-check"><input v-model="form.enabled" type="checkbox" /><label>启用该模型配置</label></div>
            <template v-if="form.model_type === 'chat_llm'">
              <div class="field"><label>模型名称</label><input v-model="form.llm.model_name" /></div>
              <div class="field"><label>API 地址</label><input v-model="form.llm.api_endpoint" /></div>
              <div class="field"><label>API Key</label><input v-model="form.llm.api_key" type="password" /></div>
              <div class="field"><label>后端格式</label><select v-model="form.llm.api_style"><option value="candle_gguf">Candle GGUF</option><option value="candle_hf">Candle HF</option><option value="open_ai_chat_completions">OpenAI Chat Completions</option><option value="open_ai_responses">OpenAI Responses</option></select></div>
            </template>
            <div v-else class="field-full"><label>本地模型目录</label><select v-model="form.local_model_name"><option value="">请选择</option><option v-for="name in localEmbeddingModels" :key="name" :value="name">{{ name }}</option></select></div>
          </div>
          <div class="panel-actions"><button class="btn ghost" @click="closeDrawer">取消</button><button class="btn primary" @click="submitForm">保存</button></div>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref } from "vue";
import type { LlmConfig } from "../../api/client";
import { useLlm } from "../composables/useLlm";

const { filteredItems, form, drawerVisible, isCreating, filters, localEmbeddingModels, startCreate, closeDrawer, editItem, submitForm, removeItem, compactId, copiedId, copyConfig, handleFileChange } = useLlm();
const importFileInput = ref<HTMLInputElement | null>(null);
function triggerImportFile() { importFileInput.value?.click(); }
function copyLlmConfig(item: LlmConfig) { copyConfig({ name: item.name, enabled: item.enabled, model: item.model }, item.config_id); }
</script>

<style scoped lang="scss">
@use "../styles/connections" as *;
@use "../styles/llm" as *;
</style>
