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
            <div class="usage-filters">
              <t-radio-group v-model="period" variant="default-filled">
                <t-radio-button value="day">天</t-radio-button>
                <t-radio-button value="three">最近3天</t-radio-button>
                <t-radio-button value="week">最近7天</t-radio-button>
                <t-radio-button value="month">月</t-radio-button>
                <t-radio-button value="custom">指定日期</t-radio-button>
              </t-radio-group>
              <t-date-range-picker v-if="period === 'custom'" v-model="dateRange" value-type="YYYY-MM-DD" :clearable="false" :allow-input="false" class="date-range" />
            </div>
          </template>
          <p class="metric-label">{{ periodDescription }} · 本地时间</p>
          <ModelUsageChart :records="summary.recent_records" :period="period" :date-range="period === 'custom' ? dateRange : undefined" />
        </t-card>
      </template>
      <t-card v-else-if="!loading && !error">模型配置不存在。</t-card>
    </t-loading>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ModelUsageChart from "./ModelUsageChart.vue";
import { useModelUsageDetail } from "./ModelUsageDetail";

const { router, model, summary, loading, error, period, dateRange, periodDescription, metrics, load } = useModelUsageDetail();
</script>

<style scoped lang="scss">
@use "./ModelUsageDetail" as *;
</style>
