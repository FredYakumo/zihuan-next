<template>
  <section class="page model-usage-page">
    <AdminPageHeader :title="model ? model.name + ' · 使用详细' : '模型使用详细'">
      <t-button variant="outline" @click="router.push('/data-explorer')">返回数据检索</t-button>
      <t-button variant="text" :loading="loading" @click="load">刷新</t-button>
    </AdminPageHeader>
    <t-alert v-if="error" theme="error" :message="error" />
    <t-loading :loading="loading">
      <template v-if="model && summary">
        <div class="metrics">
          <t-card v-for="metric in metrics" :key="metric.label" bordered><div class="metric-label">{{ metric.label }}</div><strong class="metric-value">{{ metric.value }}</strong></t-card>
        </div>
        <t-card title="Tokens 时段使用" bordered class="usage-card">
          <template #actions>
            <t-radio-group v-model="period" variant="default-filled">
              <t-radio-button value="day">天</t-radio-button>
              <t-radio-button value="three">最近3天</t-radio-button>
              <t-radio-button value="week">最近7天</t-radio-button>
              <t-radio-button value="month">月</t-radio-button>
            </t-radio-group>
          </template>
          <p class="metric-label">{{ period === "day" ? "今天，按小时统计" : period === "month" ? "最近30天，按天统计" : "包含今天，按天统计" }} · 本地时间</p>
          <ModelUsageChart :records="summary.recent_records" :period="period" />
        </t-card>
        <p class="metric-label">指标为累计使用数据；缓存命中率、速度及首 token 时间仅对有对应指标的调用取平均。流式速度按首 token 后的生成耗时计算，非流式按请求总耗时计算。未提供的指标显示为 —。</p>
      </template>
      <t-card v-else-if="!loading && !error">模型配置不存在。</t-card>
    </t-loading>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { system, type LlmConfig } from "../../api/client";
import type { ModelUsageSummary } from "../model";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ModelUsageChart from "./ModelUsageChart.vue";
const route = useRoute();
const router = useRouter();
const model = ref<LlmConfig | null>(null);
const summary = ref<ModelUsageSummary | null>(null);
const loading = ref(false);
const error = ref("");
const period = ref("three");
const metrics = computed(() => {
  const data = summary.value;
  if (!data) return [];
  return [
    { label: "总输入 Tokens", value: data.input_tokens.toLocaleString() },
    { label: "总输出 Tokens", value: data.output_tokens.toLocaleString() },
    { label: "总缓存输入", value: data.cached_input_tokens.toLocaleString() },
    { label: "平均缓存命中率", value: data.average_cache_hit_rate == null ? "—" : (data.average_cache_hit_rate * 100).toFixed(1) + "%" },
    { label: "平均 Tokens /s", value: data.average_tokens_per_second == null ? "—" : data.average_tokens_per_second.toFixed(2) },
    { label: "平均首 Token 时间", value: data.average_first_token_ms == null ? "—" : (data.average_first_token_ms / 1000).toFixed(2) + " s" },
  ];
});
let requestId = 0;
async function load() {
  const current = ++requestId;
  const id = String(route.params.modelId);
  loading.value = true;
  error.value = "";
  try {
    const [models, summaries] = await Promise.all([system.llm.list(), system.modelUsage.list()]);
    if (current !== requestId) return;
    model.value = models.find((item) => item.config_id === id) ?? null;
    summary.value = summaries.find((item) => item.config_id === id) ?? null;
  } catch (failure: unknown) { if (current === requestId) error.value = (failure as Error).message; }
  finally { if (current === requestId) loading.value = false; }
}
watch(() => route.params.modelId, load, { immediate: true });
</script>

<style scoped lang="scss">
.model-usage-page { display: grid; gap: 16px; }
.metrics { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
.metric-label { color: var(--td-text-color-secondary); font-size: 13px; }
.metric-value { display: block; margin-top: 12px; font-size: 26px; overflow-wrap: anywhere; }
.usage-card { margin-top: 16px; }
@media (max-width: 700px) { .metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
