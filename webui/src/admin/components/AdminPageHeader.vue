<template>
  <div class="admin-page-header">
    <h2 class="admin-page-header-title">{{ title }}</h2>
    <div v-if="$slots.default" class="admin-page-header-actions">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ title: string }>();
</script>

<style scoped lang="scss">
@use "./admin-page-header" as *;
</style>
