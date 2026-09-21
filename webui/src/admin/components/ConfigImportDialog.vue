<template>
  <t-dialog
    :visible="visible"
    :header="title"
    :confirm-btn="null"
    cancel-btn="取消"
    :close-on-overlay-click="false"
    @update:visible="emit('update:visible', $event)"
  >
    <div class="config-import-dialog-actions">
      <t-button block theme="primary" @click="emit('create')">{{ createLabel }}</t-button>
      <t-button block variant="outline" :loading="loading" @click="emit('clipboard-import')">从剪贴板导入</t-button>
      <t-button block variant="outline" :loading="loading" @click="fileInput?.click()">从 JSON 导入</t-button>
      <input
        ref="fileInput"
        type="file"
        accept=".json,application/json"
        class="config-import-dialog-file-input"
        @change="emit('file-change', $event)"
      />
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { ref } from "vue";

withDefaults(
  defineProps<{
    visible: boolean;
    title: string;
    createLabel?: string;
    loading?: boolean;
  }>(),
  {
    createLabel: "新增配置",
    loading: false,
  },
);

const emit = defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "create"): void;
  (event: "clipboard-import"): void;
  (event: "file-change", value: Event): void;
}>();

const fileInput = ref<HTMLInputElement | null>(null);
</script>

<style scoped lang="scss">
@use "./config-import-dialog" as *;
</style>
