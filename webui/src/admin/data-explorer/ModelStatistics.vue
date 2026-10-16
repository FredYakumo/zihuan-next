<template>
  <section class="page model-statistics-page">
    <AdminPageHeader title="模型统计"><t-button variant="outline" @click="router.push('/data-explorer')">返回数据检索</t-button></AdminPageHeader>
    <t-card title="模型统计" bordered header-bordered>
    <template #actions>
      <div class="model-toolbar">
        <t-input v-model="query" placeholder="搜索模型" clearable class="model-search">
          <template #prefix-icon><SearchIcon /></template>
        </t-input>
        <t-button variant="text" :loading="loading" @click="load">刷新</t-button>
      </div>
    </template>
    <t-alert v-if="error" theme="error" :message="error" />
    <t-loading :loading="loading">
      <div class="model-list">
        <article v-for="model in filteredModels" :key="model.config_id" class="model-row">
          <div class="model-summary">
            <div class="model-heading">
              <strong>{{ model.name }}</strong>
              <!-- <span>{{ model.model.type === "chat_llm" ? model.model.llm.model_name : model.model.model_name }}</span> -->
            </div>
            <div class="model-meta">
              <span>更新时间：{{ formatTime(model.updated_at) }}</span>
              <span>最后使用：{{ relativeTime(stats.get(model.config_id)?.last_used_at) }}</span>
              <span>Tokens 总量：{{ (stats.get(model.config_id)?.total_tokens ?? 0).toLocaleString() }}</span>
            </div>
          </div>
          <div class="model-chart">
            <ModelUsageChart :records="stats.get(model.config_id)?.recent_records ?? []" compact />
          </div>
          <t-button variant="outline" @click="router.push('/data-explorer/models/' + encodeURIComponent(model.config_id))">详细</t-button>
        </article>
        <div v-if="!loading && filteredModels.length === 0" class="empty-state">{{ models.length === 0 ? "暂无模型配置。" : "没有匹配的模型。" }}</div>
      </div>
    </t-loading>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { SearchIcon } from "tdesign-icons-vue-next";
import { system, type LlmConfig } from "../../api/client";
import { formatTime, type ModelUsageSummary } from "../model";
import ModelUsageChart from "./ModelUsageChart.vue";

const router = useRouter();
const models = ref<LlmConfig[]>([]);
const summaries = ref<ModelUsageSummary[]>([]);
const query = ref("");
const stats = computed(() => new Map(summaries.value.map((summary) => [summary.config_id, summary])));
const filteredModels = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  if (!keyword) return models.value;
  return models.value.filter((model) => {
    const modelName = model.model.type === "chat_llm" ? model.model.llm.model_name : model.model.model_name;
    return [model.name, modelName, model.config_id].some((value) => value.toLowerCase().includes(keyword));
  });
});
const loading = ref(false);
const error = ref("");
function relativeTime(value?: string | null) {
  if (!value) return "尚未使用";
  const days = Math.floor((Date.now() - new Date(value).getTime()) / 86400000);
  return days < 1 ? "最近" : `${days}天前`;
}
async function load() {
  loading.value = true;
  error.value = "";
  try {
    [models.value, summaries.value] = await Promise.all([system.llm.list(), system.modelUsage.list()]);
  } catch (failure: unknown) { error.value = (failure as Error).message; }
  finally { loading.value = false; }
}
onMounted(load);
</script>

<style scoped lang="scss">
.model-list { display: grid; gap: 12px; }
.model-statistics-page { display: grid; gap: 16px; }
.model-toolbar { display: flex; align-items: center; gap: 8px; }
.model-search { width: 220px; max-width: 100%; }
.model-row { display: grid; grid-template-columns: minmax(200px, 1fr) minmax(0, 2fr) auto; gap: 20px; align-items: center; padding: 12px 16px; border: 1px solid var(--td-component-stroke); border-radius: var(--td-radius-medium); }
.model-summary { display: grid; gap: 6px; overflow-wrap: anywhere; }
.model-heading { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; }
.model-meta { display: flex; flex-wrap: wrap; gap: 2px 12px; }
.model-heading span, .model-meta span { color: var(--td-text-color-secondary); font-size: 13px; }
.model-chart { min-width: 0; }
.empty-state { padding: 32px; text-align: center; color: var(--td-text-color-placeholder); }
@media (max-width: 800px) { .model-row { grid-template-columns: 1fr; gap: 12px; } }
</style>
