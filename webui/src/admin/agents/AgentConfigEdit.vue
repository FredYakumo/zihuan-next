<template>
  <section class="page agent-config-page agent-config-edit-page">
    <AdminPageHeader :breadcrumbs="breadcrumbs">
      <t-button variant="outline" @click="backToList">返回列表</t-button>
      <t-button theme="primary" :loading="saving" @click="save">保存</t-button>
    </AdminPageHeader>

    <t-alert v-if="listError" theme="error" :message="listError" class="agent-config-alert" />
    <t-loading :loading="initializing" show-overlay>
      <t-card :bordered="true">
        <AgentConfigForm :page="page" />
      </t-card>
    </t-loading>

    <ServiceRestartConfirmDialog :state="restartDialog.state" @close="restartDialog.close()" @save-only="restartDialog.saveOnly()" @restart="restartDialog.restartAll()" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ServiceRestartConfirmDialog from "../components/ServiceRestartConfirmDialog.vue";
import AgentConfigForm from "./AgentConfigForm.vue";
import { useAgentConfig } from "./AgentConfig";

const route = useRoute();
const router = useRouter();
const page = useAgentConfig({ autoLoad: false });
const {
  form,
  isCreating,
  saving,
  load,
  startCreate,
  openEditor,
  save,
  restartDialog,
  listError,
} = page;

const initializing = ref(true);

function queryString(key: string): string {
  const value = route.query[key];
  return typeof value === "string" ? value : "";
}

const breadcrumbs = computed(() => [
  { label: "Agent 配置", to: "/agent-config" },
  { label: isCreating.value ? "新增 SubAgent" : `${form.id} 配置` },
]);

function backToList() {
  void router.push("/agent-config");
}

onMounted(async () => {
  try {
    await load();
    const id = queryString("id");
    if (id) {
      await openEditor(id);
      if (listError.value) {
        alert(`Agent 加载失败: ${listError.value}`);
        await router.replace("/agent-config");
        return;
      }
    } else {
      startCreate();
    }
  } finally {
    initializing.value = false;
  }
});
</script>

<style scoped lang="scss">
@use "./AgentConfig" as *;
</style>
