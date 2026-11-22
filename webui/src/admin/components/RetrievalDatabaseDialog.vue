<template>
  <t-dialog
    :visible="visible"
    header="添加检索数据库连接"
    :confirm-btn="null"
    cancel-btn="关闭"
    width="520px"
    @update:visible="emit('update:visible', $event)"
  >
    <p>连接已有的 Weaviate 或 Elasticsearch。需要部署服务时，请使用 Zihuan Composer。</p>
    <t-space direction="vertical" style="width: 100%">
      <t-button block theme="primary" @click="emit('create-page')">前往连接页创建</t-button>
      <t-button block variant="outline" @click="emit('clipboard-import')">从剪贴板导入</t-button>
      <t-button block variant="outline" @click="fileInput?.click()">从 JSON 导入</t-button>
      <input ref="fileInput" type="file" accept=".json,application/json" hidden @change="emit('file-change', $event)" />
    </t-space>
  </t-dialog>
</template>

<script setup lang="ts">
import { ref } from "vue";
import type { ConnectionConfig } from "../../api/client";

defineProps<{ visible: boolean }>();
const fileInput = ref<HTMLInputElement | null>(null);
const emit = defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "created", connection: ConnectionConfig): void;
  (event: "clipboard-import"): void;
  (event: "file-change", value: Event): void;
  (event: "create-page"): void;
}>();
</script>
