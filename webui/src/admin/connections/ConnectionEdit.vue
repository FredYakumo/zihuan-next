<template>
  <section class="page connections-page connection-edit-page">
    <AdminPageHeader :breadcrumbs="breadcrumbs">
      <t-button variant="outline" @click="openImportDialog">导入配置</t-button>
      <t-button variant="outline" :disabled="saving" @click="closeDrawer">返回列表</t-button>
      <t-button theme="primary" :loading="saving" @click="submitForm">
        {{ isCreating ? "创建连接" : "保存修改" }}
      </t-button>
    </AdminPageHeader>

    <t-loading :loading="initializing" show-overlay>
      <t-card :bordered="true">
        <ConnectionForm :page="page" />
      </t-card>
    </t-loading>

    <t-dialog
      v-model:visible="importDialogVisible"
      header="导入连接配置"
      width="640px"
      :close-on-overlay-click="false"
      :confirm-btn="{ content: '导入', disabled: !importJsonText.trim() }"
      cancel-btn="取消"
      @confirm="submitImport"
    >
      <div class="import-dialog-body">
        <div
          class="import-dropzone"
          :class="{ 'import-dropzone--active': importDragging }"
          role="button"
          tabindex="0"
          @click="importFileInput?.click()"
          @keydown.enter.prevent="importFileInput?.click()"
          @keydown.space.prevent="importFileInput?.click()"
          @dragover.prevent="importDragging = true"
          @dragleave.prevent="handleImportDragLeave"
          @drop.prevent="handleImportDrop"
        >
          <strong>拖拽 JSON 文件到此处</strong>
          <span>或点击选择 JSON 文件</span>
        </div>
        <input
          ref="importFileInput"
          type="file"
          accept=".json,application/json"
          class="connection-import-input"
          @change="handleImportFileChange"
        />
        <t-textarea
          v-model="importJsonText"
          class="import-json-textarea"
          :autosize="{ minRows: 10, maxRows: 18 }"
          placeholder="也可以在此直接输入连接配置 JSON"
        />
      </div>
    </t-dialog>

    <ServiceRestartConfirmDialog :state="restartDialog.state" @close="restartDialog.close()" @save-only="restartDialog.saveOnly()" @restart="restartDialog.restartAll()" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ServiceRestartConfirmDialog from "../components/ServiceRestartConfirmDialog.vue";
import ConnectionForm from "./ConnectionForm.vue";
import { useConnections } from "./useConnections";

const route = useRoute();
const router = useRouter();
const page = useConnections({ autoLoad: false });
const {
  form,
  isCreating,
  saving,
  load,
  startCreate,
  editConnection,
  duplicateConnection,
  submitForm,
  closeDrawer,
  restartDialog,
  importDialogVisible,
  importJsonText,
  importDragging,
  importFileInput,
  openImportDialog,
  handleImportFileChange,
  handleImportDragLeave,
  handleImportDrop,
  submitImport,
  connections,
} = page;

const initializing = ref(true);

function queryString(key: string): string {
  const value = route.query[key];
  return typeof value === "string" ? value : "";
}

const breadcrumbs = computed(() => [
  { label: "连接配置", to: "/connections" },
  { label: isCreating.value ? "新建连接" : "编辑连接" },
]);

onMounted(async () => {
  try {
    await load();
    const id = queryString("id");
    const copyId = queryString("copy");
    const type = queryString("type") as Parameters<typeof startCreate>[0];
    if (id) {
      const item = connections.value.find((connection) => connection.config_id === id);
      if (item) {
        editConnection(item);
      } else {
        alert("连接配置不存在或已被删除");
        await router.replace("/connections");
        return;
      }
    } else if (copyId) {
      const item = connections.value.find((connection) => connection.config_id === copyId);
      if (item) {
        duplicateConnection(item);
      } else {
        alert("连接配置不存在或已被删除");
        await router.replace("/connections");
        return;
      }
    } else {
      startCreate(type);
    }
    if (route.query.import === "1") {
      openImportDialog();
    }
  } catch (error) {
    alert(`连接配置加载失败: ${(error as Error).message}`);
  } finally {
    initializing.value = false;
  }
});
</script>

<style scoped lang="scss">
@use "./connections" as *;
</style>
