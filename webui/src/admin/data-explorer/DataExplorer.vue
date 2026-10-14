<template>
  <section class="page data-explorer-page">
    <AdminPageHeader title="数据检索" />
    <div class="explorer-entry-grid">
      <t-button class="explorer-entry" theme="primary" block @click="open('/data-explorer/models')">模型统计</t-button>
      <t-button class="explorer-entry" theme="primary" block @click="open('/data-explorer/services')">Service</t-button>
    </div>
  </section>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";

const router = useRouter();
function open(path: string) { void router.push(path); }
</script>

<style scoped lang="scss">
.data-explorer-page { display: grid; gap: 16px; }
.explorer-entry-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }
.explorer-entry { height: 180px; border-radius: 20px; font-size: 24px; font-weight: 600; }
@media (max-width: 720px) { .explorer-entry-grid { grid-template-columns: 1fr; } }
</style>
