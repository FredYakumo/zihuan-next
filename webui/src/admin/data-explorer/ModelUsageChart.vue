<template><div ref="element" class="model-usage-chart" :class="{ compact }" /></template>

<script setup lang="ts">
import { useModelUsageChart, type ModelUsageChartProps } from "./ModelUsageChart";

const props = defineProps<ModelUsageChartProps>();
const { element } = useModelUsageChart(props);
</script>

<style scoped lang="scss">
@use "./ModelUsageChart" as *;
</style>
