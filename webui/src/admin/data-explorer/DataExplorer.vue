<template>
  <section class="page data-explorer-page">
    <AdminPageHeader title="数据检索" />
    <nav class="explorer-entry-grid" aria-label="数据检索入口">
      <RouterLink class="explorer-entry" to="/data-explorer/models">
        <span class="entry-icon" aria-hidden="true"><ChartBarIcon /></span>
        <span class="entry-title">模型统计</span>
        <ChevronRightIcon class="entry-arrow" aria-hidden="true" />
      </RouterLink>
      <RouterLink class="explorer-entry" to="/data-explorer/services">
        <span class="entry-icon" aria-hidden="true"><ServerIcon /></span>
        <span class="entry-title">Service 数据</span>
        <ChevronRightIcon class="entry-arrow" aria-hidden="true" />
      </RouterLink>
    </nav>
  </section>
</template>

<script setup lang="ts">
import { ChartBarIcon, ChevronRightIcon, ServerIcon } from "tdesign-icons-vue-next";
import { RouterLink } from "vue-router";
</script>

<style scoped lang="scss">
@use "./DataExplorer" as *;
</style>
