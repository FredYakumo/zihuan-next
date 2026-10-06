<template>
  <t-select
    v-model="model"
    :placeholder="placeholder"
    :clearable="clearable"
    :filterable="filterable"
    :disabled="disabled"
    @change="onChange"
  >
    <!-- Callers prepend special options (e.g. entry points into a create dialog) here. -->
    <slot />
    <t-option v-for="item in options" :key="item.value" :value="item.value" :label="item.label">
      <span>{{ item.label }}</span>
      <span v-if="item.description" class="common-select-option-desc">{{ item.description }}</span>
    </t-option>
  </t-select>
</template>

<script setup lang="ts">
import type { PropType } from "vue";

interface CommonSelectOption {
  value: string;
  label: string;
  /** Optional secondary text rendered right-aligned inside the dropdown list. */
  description?: string;
}

const props = defineProps({
  options: { type: Array as PropType<CommonSelectOption[]>, default: () => [] },
  placeholder: { type: String, default: undefined },
  clearable: { type: Boolean, default: false },
  filterable: { type: Boolean, default: false },
  disabled: { type: Boolean, default: false },
});

const emit = defineEmits<{
  change: [value: string | undefined];
}>();

const model = defineModel<string>({ required: true });

function onChange(value: unknown) {
  emit("change", typeof value === "string" ? value : undefined);
}
</script>

<style scoped>
.common-select-option-desc {
  float: right;
  color: var(--td-text-color-placeholder);
  font-size: 12px;
  margin-left: 12px;
}
</style>
