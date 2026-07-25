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
.admin-page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 14px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--admin-border);
}

.admin-page-header-title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.admin-page-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

@media (max-width: 900px) {
  .admin-page-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }
}
</style>
