<template>
  <label class="component-header">
    <input v-model="enabled" type="checkbox" />
    <strong>{{ title }}</strong>
    <small v-if="hint">{{ hint }}</small>
  </label>
</template>

<script setup lang="ts">
const enabled = defineModel<boolean>("enabled", { required: true });
defineProps<{ title: string; hint?: string }>();
</script>
