<template>
  <t-card title="模型统计" bordered header-bordered>
    <template #actions><t-button variant="text" :loading="loading" @click="load">刷新</t-button></template>
    <p class="statistics-note">统计从启用此功能后的调用开始积累，Tokens 仅汇总模型返回的使用量。</p>
    <t-alert v-if="error" theme="error" :message="error" />
    <t-loading :loading="loading">
      <div class="model-list">
        <article v-for="model in models" :key="model.config_id" class="model-row">
          <div class="model-summary">
            <strong>{{ model.name }}</strong>
            <span>{{ model.model.type === "chat_llm" ? model.model.llm.model_name : model.model.model_name }}</span>
            <span>更新时间：{{ formatTime(model.updated_at) }}</span>
            <span>最后使用：{{ relativeTime(stats.get(model.config_id)?.last_used_at) }}</span>
            <span>Tokens 总量：{{ (stats.get(model.config_id)?.total_tokens ?? 0).toLocaleString() }}</span>
          </div>
          <div class="model-chart">
            <span class="chart-label">最近 3 天（上午 0–12、下午 12–18、晚上 18–24）</span>
            <ModelUsageChart :records="stats.get(model.config_id)?.recent_records ?? []" compact />
          </div>
          <t-button variant="outline" @click="router.push('/data-explorer/models/' + encodeURIComponent(model.config_id))">详细</t-button>
        </article>
        <div v-if="!loading && models.length === 0" class="empty-state">暂无模型配置。</div>
      </div>
    </t-loading>
  </t-card>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { system, type LlmConfig } from "../../api/client";
import { formatTime, type ModelUsageSummary } from "../model";
import ModelUsageChart from "./ModelUsageChart.vue";

const router = useRouter();
const models = ref<LlmConfig[]>([]);
const summaries = ref<ModelUsageSummary[]>([]);
const stats = computed(() => new Map(summaries.value.map((summary) => [summary.config_id, summary])));
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
.model-list { display: grid; gap: 16px; }
.statistics-note { color: var(--td-text-color-secondary); font-size: 13px; margin: 0 0 16px; }
.model-row { display: grid; grid-template-columns: minmax(200px, 1fr) minmax(260px, 2fr) auto; gap: 24px; align-items: center; padding: 16px 0; border-bottom: 1px solid var(--td-component-stroke); }
.model-row:last-child { border-bottom: 0; }
.model-summary { display: grid; gap: 8px; overflow-wrap: anywhere; }
.model-summary span, .chart-label { color: var(--td-text-color-secondary); font-size: 13px; }
.model-chart { min-width: 0; }
.empty-state { padding: 32px; text-align: center; color: var(--td-text-color-placeholder); }
@media (max-width: 800px) { .model-row { grid-template-columns: 1fr; gap: 12px; } }
</style>
