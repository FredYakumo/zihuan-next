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

import { useCommonSelect, type CommonSelectOption } from "./CommonSelect";

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

const { onChange } = useCommonSelect(props, emit);
</script>

<style scoped lang="scss">
@use "./CommonSelect" as *;
</style>
