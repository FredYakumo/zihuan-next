<template>
  <section class="page llm-page llm-edit-page">
    <AdminPageHeader :breadcrumbs="breadcrumbs">
      <t-button variant="outline" @click="triggerImportFile">导入配置</t-button>
      <input ref="importFileInput" type="file" accept=".json,application/json" class="llm-import-input" @change="handleFileChange" />
      <t-button variant="outline" @click="closeDrawer">返回列表</t-button>
      <t-button theme="primary" @click="submitForm">{{ isCreating ? "创建模型" : "保存修改" }}</t-button>
    </AdminPageHeader>

    <t-alert v-if="route.query.import === '1'" theme="info" message="请点击右上角的「导入配置」选择 JSON 文件。" />

    <t-loading :loading="initializing" show-overlay>
      <t-card :bordered="true">
        <LlmForm :page="page" />
      </t-card>
    </t-loading>

    <ModelDownloadDialog v-model:visible="downloadDialogVisible" @completed="onEmbeddingDownloadCompleted" />
    <ServiceRestartConfirmDialog :state="restartDialog.state" @close="restartDialog.close()" @save-only="restartDialog.saveOnly()" @restart="restartDialog.restartAll()" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ModelDownloadDialog from "../components/ModelDownloadDialog.vue";
import ServiceRestartConfirmDialog from "../components/ServiceRestartConfirmDialog.vue";
import LlmForm from "./LlmForm.vue";
import { useLlm } from "./useLlm";

const route = useRoute();
const router = useRouter();
const page = useLlm({ autoLoad: false });
const {
  form,
  items,
  isCreating,
  load,
  startCreate,
  editItem,
  submitForm,
  closeDrawer,
  triggerImportFile,
  importFileInput,
  handleFileChange,
  downloadDialogVisible,
  onEmbeddingDownloadCompleted,
  restartDialog,
} = page;

const initializing = ref(true);

function queryString(key: string): string {
  const value = route.query[key];
  return typeof value === "string" ? value : "";
}

const breadcrumbs = computed(() => [
  { label: "模型配置", to: "/llm" },
  { label: isCreating.value ? "新建模型配置" : "编辑模型配置" },
]);

onMounted(async () => {
  try {
    await load();
    const id = queryString("id");
    if (id) {
      const item = items.value.find((entry) => entry.config_id === id);
      if (item) {
        editItem(item);
      } else {
        alert("模型配置不存在或已被删除");
        await router.replace("/llm");
        return;
      }
    } else {
      startCreate();
    }
    if (route.query.import === "1") {
      triggerImportFile();
    }
  } catch (error) {
    alert(`模型配置加载失败: ${(error as Error).message}`);
  } finally {
    initializing.value = false;
  }
});
</script>

<style scoped lang="scss">
@use "./llm" as *;
</style>
