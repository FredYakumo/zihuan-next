<template>
  <div class="admin-page-header">
    <AdminBreadcrumb v-if="breadcrumbs?.length" :items="breadcrumbs" />
    <h2 v-else class="admin-page-header-title">{{ title }}</h2>
    <div v-if="$slots.default" class="admin-page-header-actions">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
import AdminBreadcrumb from "./AdminBreadcrumb.vue";

defineProps<{
  title?: string;
  breadcrumbs?: { label: string; to?: string }[];
}>();
</script>

<style scoped lang="scss">
@use "./admin-page-header" as *;
</style>
