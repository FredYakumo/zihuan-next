import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { system, type LlmConfig } from "../../api/client";
import type { ModelUsageSummary } from "../model";

export function useModelUsageDetail() {
  const route = useRoute();
  const router = useRouter();
  const model = ref<LlmConfig | null>(null);
  const summary = ref<ModelUsageSummary | null>(null);
  const loading = ref(false);
  const error = ref("");
  const period = ref("three");
  const today = new Date();
  const dateRange = ref<string[]>([2, 0].map((daysAgo) => {
    const date = new Date(today.getFullYear(), today.getMonth(), today.getDate() - daysAgo);
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  }));
  const periodDescription = computed(() => period.value === "custom"
    ? `${dateRange.value[0]} 至 ${dateRange.value[1]}，${dateRange.value[0] === dateRange.value[1] ? "按小时统计" : "按天统计"}`
    : period.value === "day" ? "今天，按小时统计" : period.value === "month" ? "最近30天，按天统计" : "包含今天，按天统计");
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
      let range: { start: string; end: string } | undefined;
      if (period.value === "custom") {
        const start = new Date(`${dateRange.value[0]}T00:00:00`);
        const end = new Date(`${dateRange.value[1]}T00:00:00`);
        end.setDate(end.getDate() + 1);
        range = { start: start.toISOString(), end: end.toISOString() };
      }
      const [models, summaries] = await Promise.all([system.llm.list(), system.modelUsage.list(range)]);
      if (current !== requestId) return;
      model.value = models.find((item) => item.config_id === id) ?? null;
      summary.value = summaries.find((item) => item.config_id === id) ?? null;
    } catch (failure: unknown) { if (current === requestId) error.value = (failure as Error).message; }
    finally { if (current === requestId) loading.value = false; }
  }
  watch(() => route.params.modelId, load, { immediate: true });
  watch([period, dateRange], ([currentPeriod], [previousPeriod]) => {
    if (currentPeriod === "custom" || previousPeriod === "custom") void load();
  }, { deep: true });

  return { router, model, summary, loading, error, period, dateRange, periodDescription, metrics, load };
}
