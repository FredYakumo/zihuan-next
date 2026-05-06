<template>
  <section class="page">
    <div class="page-hero">
      <h2>模型配置</h2>
      <div class="hero-actions connection-hero-actions">
        <button class="btn primary connection-hero-add-btn" @click="startCreate">+</button>
      </div>
    </div>

    <div v-if="showCreatePicker" class="connection-picker-backdrop" @click="closeCreatePicker">
      <div class="connection-picker-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>新建模型配置</h3>
          <button class="btn ghost connection-card-compact-btn" @click="closeCreatePicker">
            {{ showCreateForm ? "关闭" : "取消" }}
          </button>
        </div>

        <div v-if="showCreateForm" class="connection-picker-form">
          <div class="form-grid">
            <div class="field">
              <label>名称</label>
              <input v-model="form.name" placeholder="例如：OpenAI 主模型" />
            </div>
            <div class="field-full field-check">
              <input id="llm-enabled" v-model="form.enabled" type="checkbox" />
              <label for="llm-enabled">启用该模型配置</label>
            </div>
            <div class="field-full field-check">
              <input id="llm-multimodal-enabled" v-model="form.llm.supports_multimodal_input" type="checkbox" />
              <label for="llm-multimodal-enabled">多模态模型（允许传入图片）</label>
            </div>
            <div class="field"><label>Model Name</label><input v-model="form.llm.model_name" /></div>
            <div class="field"><label>API Endpoint</label><input v-model="form.llm.api_endpoint" /></div>
            <div class="field"><label>API Key</label><input v-model="form.llm.api_key" type="password" /></div>
            <div class="field"><label>Timeout Secs</label><input v-model.number="form.llm.timeout_secs" type="number" min="1" /></div>
            <div class="field"><label>Retry Count</label><input v-model.number="form.llm.retry_count" type="number" min="0" /></div>
          </div>
          <div class="panel-actions connection-picker-form-actions">
            <button class="btn ghost" @click="showCreateForm = false">返回</button>
            <button class="btn primary" @click="submitForm">创建模型配置</button>
          </div>
        </div>

        <div v-else class="connection-picker-grid connection-picker-grid--single">
          <button class="connection-picker-option" @click="showCreateForm = true">
            <strong>模型配置</strong>
            <span>填写模型名、接口地址、API Key 与超时重试参数</span>
          </button>
        </div>
      </div>
    </div>

    <section v-if="items.length > 0" class="panel">
      <div class="connection-grid" style="margin-top: 0;">
        <article
          v-for="item in items"
          :key="item.id"
          :class="['connection-card', { 'connection-card--editing': form.id === item.id }]"
        >
          <template v-if="form.id === item.id">
            <div class="connection-card-header connection-card-header--stacked">
              <div class="connection-card-header-top">
                <div class="connection-card-badges">
                  <span class="badge">model</span>
                  <span class="badge" :class="form.enabled ? 'success' : ''">{{ form.enabled ? "已启用" : "已停用" }}</span>
                </div>
                <div class="inline-actions connection-card-edit-actions">
                  <button class="btn primary connection-card-compact-btn" @click="submitForm">保存</button>
                  <button class="btn ghost connection-card-compact-btn" @click="resetForm">取消</button>
                </div>
              </div>
              <div class="connection-card-title-edit">
                <input v-model="form.name" class="connection-card-inline-input connection-card-inline-input--title" />
              </div>
            </div>

            <div class="connection-card-body">
              <div class="key-value connection-card-edit-row">
                <strong>启用</strong>
                <label class="connection-card-inline-check">
                  <input :id="`llm-enabled-${item.id}`" v-model="form.enabled" type="checkbox" />
                  <span>{{ form.enabled ? "已启用" : "已停用" }}</span>
                </label>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>多模态</strong>
                <label class="connection-card-inline-check">
                  <input
                    :id="`llm-multimodal-enabled-${item.id}`"
                    v-model="form.llm.supports_multimodal_input"
                    type="checkbox"
                  />
                  <span>{{ form.llm.supports_multimodal_input ? "已启用" : "未启用" }}</span>
                </label>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>Model</strong>
                <input v-model="form.llm.model_name" class="connection-card-inline-input" />
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>Endpoint</strong>
                <input v-model="form.llm.api_endpoint" class="connection-card-inline-input" />
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>API Key</strong>
                <input v-model="form.llm.api_key" class="connection-card-inline-input" type="password" />
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>Timeout</strong>
                <input v-model.number="form.llm.timeout_secs" class="connection-card-inline-input" type="number" min="1" />
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>Retry</strong>
                <input v-model.number="form.llm.retry_count" class="connection-card-inline-input" type="number" min="0" />
              </div>
            </div>
          </template>

          <template v-else>
            <div class="connection-card-header connection-card-header--stacked">
              <div class="connection-card-header-top">
                <div class="connection-card-badges">
                  <span class="badge">model</span>
                  <span class="badge" :class="item.enabled ? 'success' : ''">{{ item.enabled ? "已启用" : "已停用" }}</span>
                </div>
                <div class="inline-actions connection-card-display-actions">
                  <button class="btn ghost connection-card-compact-btn" @click="editItem(item)">编辑</button>
                  <button class="btn warn connection-card-compact-btn" @click="removeItem(item.id)">删除</button>
                </div>
              </div>
              <h4>{{ item.name }}</h4>
            </div>

            <div class="connection-card-body">
              <div class="key-value"><strong>Model</strong><span>{{ item.llm.model_name }}</span></div>
              <div class="key-value"><strong>Endpoint</strong><span class="mono">{{ item.llm.api_endpoint }}</span></div>
              <div class="key-value"><strong>多模态</strong><span>{{ item.llm.supports_multimodal_input ? "是" : "否" }}</span></div>
              <div class="key-value"><strong>Timeout</strong><span>{{ item.llm.timeout_secs }}s</span></div>
              <div class="key-value"><strong>Retry</strong><span>{{ item.llm.retry_count }} 次</span></div>
            </div>

            <div class="connection-card-footer">
              <span class="muted">更新于 {{ formatTime(item.updated_at) }}</span>
            </div>
          </template>
        </article>
      </div>
    </section>
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";

import { system, type LlmConfig } from "../../api/client";
import { defaultLlmForm, formatTime, llmFormFromConfig, type LlmFormState } from "../model";

const items = ref<LlmConfig[]>([]);
const form = reactive<LlmFormState>(defaultLlmForm());
const showCreatePicker = ref(false);
const showCreateForm = ref(false);

function resetCreateForm() {
  Object.assign(form, defaultLlmForm());
}

function startCreate() {
  resetCreateForm();
  showCreatePicker.value = true;
  showCreateForm.value = false;
}

function closeCreatePicker() {
  resetCreateForm();
  showCreatePicker.value = false;
  showCreateForm.value = false;
}

async function load() {
  items.value = await system.llm.list();
}

function editItem(item: LlmConfig) {
  Object.assign(form, llmFormFromConfig(item));
  showCreatePicker.value = false;
  showCreateForm.value = false;
}

async function submitForm() {
  if (!form.name.trim() || !form.llm.model_name.trim() || !form.llm.api_endpoint.trim()) {
    alert("请至少填写名称、模型名和 API Endpoint");
    return;
  }
  const payload = {
    name: form.name.trim(),
    enabled: form.enabled,
    llm: {
      ...form.llm,
      api_key: form.llm.api_key?.trim() || null,
    },
  };
  if (form.id) {
    await system.llm.update(form.id, payload);
  } else {
    await system.llm.create(payload);
  }
  resetCreateForm();
  closeCreatePicker();
  await load();
}

async function removeItem(id: string) {
  if (!window.confirm("确认删除这个模型配置吗？")) {
    return;
  }
  await system.llm.delete(id);
  if (form.id === id) {
    resetCreateForm();
  }
  await load();
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`模型配置加载失败: ${(error as Error).message}`);
  });
});
</script>
