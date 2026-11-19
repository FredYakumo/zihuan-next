<template>
  <nav class="admin-breadcrumb" aria-label="页面路径">
    <template v-for="(item, index) in items" :key="`${item.label}-${index}`">
      <button
        v-if="item.to && index < items.length - 1"
        type="button"
        class="admin-breadcrumb-link"
        @click="navigate(item.to)"
      >
        {{ item.label }}
      </button>
      <span
        v-else
        :class="index === items.length - 1 ? 'admin-breadcrumb-current' : 'admin-breadcrumb-text'"
        :aria-current="index === items.length - 1 ? 'page' : undefined"
      >
        {{ item.label }}
      </span>
      <span v-if="index < items.length - 1" class="admin-breadcrumb-separator" aria-hidden="true">/</span>
    </template>
  </nav>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";

defineProps<{
  items: { label: string; to?: string }[];
}>();

const router = useRouter();

function navigate(to: string) {
  void router.push(to);
}
</script>

<style scoped lang="scss">
@use "./admin-breadcrumb" as *;
</style>
