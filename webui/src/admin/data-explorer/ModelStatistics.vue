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
import { SearchIcon } from "tdesign-icons-vue-next";
import ModelUsageChart from "./ModelUsageChart.vue";
import { useModelStatistics } from "./ModelStatistics";

const { router, models, query, stats, filteredModels, loading, error, relativeTime, load, formatTime } = useModelStatistics();
</script>

<style scoped lang="scss">
@use "./ModelStatistics" as *;
</style>