<template><div ref="element" class="model-usage-chart" :class="{ compact }" /></template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { init, use, type ECharts, type EChartsCoreOption } from "echarts/core";
import { BarChart } from "echarts/charts";
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { ModelUsageRecord } from "../model";

use([BarChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);
const props = defineProps<{ records: ModelUsageRecord[]; period?: string; compact?: boolean; dateRange?: string[] }>();
const element = ref<HTMLDivElement | null>(null);
let chart: ECharts | undefined;
let observer: ResizeObserver | undefined;

function render() {
  if (!chart || !element.value) return;
  const days = props.compact ? 3 : props.period === "day" ? 1 : props.period === "week" ? 7 : props.period === "month" ? 30 : 3;
  const now = new Date();
  const customRange = !props.compact && props.period === "custom" && props.dateRange?.length === 2 ? props.dateRange : undefined;
  const start = customRange ? new Date(`${customRange[0]}T00:00:00`) : new Date(now.getFullYear(), now.getMonth(), now.getDate() - days + 1);
  const rangeEnd = customRange ? new Date(`${customRange[1]}T00:00:00`) : new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const buckets: { start: number; end: number; label: string; cached: number; missed: number; output: number; total: number }[] = [];
  const hourly = !props.compact && start.getTime() === rangeEnd.getTime();
  const slots = hourly ? 24 : props.compact ? 3 : 1;
  for (let day = 0; new Date(start.getFullYear(), start.getMonth(), start.getDate() + day) <= rangeEnd; day++) {
    for (let slot = 0; slot < slots; slot++) {
      const hours = props.compact ? [0, 12, 18, 24] : Array.from({ length: slots + 1 }, (_, index) => index * (24 / slots));
      const date = new Date(start.getFullYear(), start.getMonth(), start.getDate() + day, hours[slot]);
      const end = new Date(start.getFullYear(), start.getMonth(), start.getDate() + day, hours[slot + 1]);
      const label = hourly ? `${slot}:00` : `${date.getMonth() + 1}/${date.getDate()}${props.compact ? [" 上午", " 下午", " 晚上"][slot] : ""}`;
      buckets.push({ start: date.getTime(), end: end.getTime(), label, cached: 0, missed: 0, output: 0, total: 0 });
    }
  }
  for (const record of props.records) {
    const timestamp = new Date(record.used_at).getTime();
    if (timestamp > now.getTime()) continue;
    const bucket = buckets.find((bucket) => timestamp >= bucket.start && timestamp < bucket.end);
    if (!bucket || !record.usage) continue;
    const usage = record.usage;
    const input = usage.prompt_tokens ?? 0;
    const cached = Math.min(input, usage.cached_prompt_tokens ?? 0);
    const output = usage.completion_tokens ?? 0;
    bucket.cached += cached;
    bucket.missed += usage.prompt_cache_miss_tokens ?? Math.max(0, input - cached);
    bucket.output += output;
    bucket.total += usage.total_tokens ?? input + output;
  }
  const styles = getComputedStyle(element.value);
  const textColor = styles.getPropertyValue("--td-text-color-secondary").trim() || "#777";
  const option: EChartsCoreOption = {
    color: ["#00a870", "#0052d9", "#ed7b2f"],
    textStyle: { color: textColor },
    tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
    legend: props.compact ? { show: false } : { top: 0, textStyle: { color: textColor } },
    grid: { left: 12, right: 12, top: props.compact ? 15 : 45, bottom: 10, containLabel: true },
    xAxis: { type: "category", data: buckets.map((bucket) => bucket.label), axisLabel: { color: textColor, rotate: props.compact ? 25 : 0 } },
    yAxis: { type: "value", minInterval: 1, axisLabel: { color: textColor }, splitLine: { lineStyle: { opacity: 0.15 } } },
    series: props.compact
      ? [{ name: "Tokens", type: "bar", data: buckets.map((bucket) => bucket.total), barMaxWidth: 28 }]
      : [
        { name: "缓存输入", type: "bar", stack: "tokens", data: buckets.map((bucket) => bucket.cached), barMaxWidth: 42 },
        { name: "未命中缓存输入", type: "bar", stack: "tokens", data: buckets.map((bucket) => bucket.missed), barMaxWidth: 42 },
        { name: "输出", type: "bar", stack: "tokens", data: buckets.map((bucket) => bucket.output), barMaxWidth: 42 },
      ],
  };
  chart.setOption(option, true);
}

onMounted(() => {
  if (!element.value) return;
  chart = init(element.value);
  observer = new ResizeObserver(() => { chart?.resize(); });
  observer.observe(element.value);
  render();
});
watch(() => [props.records, props.period, props.compact, props.dateRange], render, { deep: true });
onBeforeUnmount(() => { observer?.disconnect(); chart?.dispose(); });
</script>

<style scoped>
.model-usage-chart { width: 100%; height: 380px; }
.model-usage-chart.compact { height: 190px; }
</style>
