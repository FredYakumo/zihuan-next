<template>
  <section class="page plugins-page">
    <AdminPageHeader title="插件" />

    <t-card title="已安装插件" bordered header-bordered>
      <template #actions>
        <t-button theme="primary" @click="openCreateDialog">新增插件</t-button>
      </template>

      <t-table
        :data="plugins"
        :columns="columns"
        row-key="name"
        :loading="loading"
        :pagination="false"
        size="small"
      >
        <template #extra_install_metadata="{ row }">
          <code class="metadata-preview">{{ formatMetadata(row.extra_install_metadata) }}</code>
        </template>
        <template #actions="{ row }">
          <t-space size="small">
            <t-button variant="text" @click="openEditDialog(row)">编辑</t-button>
            <t-popconfirm :content="`确认删除插件“${row.name}”？`" @confirm="removePlugin(row.name)">
              <t-button variant="text" theme="danger">删除</t-button>
            </t-popconfirm>
          </t-space>
        </template>
      </t-table>
    </t-card>

    <t-dialog
      v-model:visible="dialogVisible"
      :header="editingName ? '编辑插件' : '新增插件'"
      :confirm-btn="{ content: '保存', loading: saving }"
      @confirm="savePlugin"
    >
      <t-form label-align="top">
        <t-form-item label="插件名称" required>
          <t-input v-model="form.name" placeholder="例如：示例插件" />
        </t-form-item>
        <t-form-item label="版本号" required>
          <t-input v-model="form.version" placeholder="例如：1.0.0" />
        </t-form-item>
        <t-form-item label="安装日期" required>
          <t-date-picker v-model="form.installed_at" format="YYYY-MM-DD" value-type="YYYY-MM-DD" />
        </t-form-item>
        <t-form-item label="安装方式" required>
          <t-input v-model="form.installation_method" placeholder="例如：手动安装、市场安装" />
        </t-form-item>
        <t-form-item label="额外安装信息元数据（JSON）" required>
          <t-textarea v-model="form.extra_install_metadata" :autosize="{ minRows: 5 }" />
        </t-form-item>
      </t-form>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";

import AdminPageHeader from "../components/AdminPageHeader.vue";
import { pluginsApi, type PluginRecord } from "../../api/client";

interface PluginForm {
  name: string;
  version: string;
  installed_at: string;
  installation_method: string;
  extra_install_metadata: string;
}

const plugins = ref<PluginRecord[]>([]);
const loading = ref(false);
const saving = ref(false);
const dialogVisible = ref(false);
const editingName = ref<string | null>(null);
const form = reactive<PluginForm>(emptyForm());

const columns = [
  { colKey: "name", title: "插件名称" },
  { colKey: "version", title: "版本号", width: 120 },
  { colKey: "installed_at", title: "安装日期", width: 140 },
  { colKey: "installation_method", title: "安装方式", width: 160 },
  { colKey: "extra_install_metadata", title: "额外安装信息", ellipsis: true },
  { colKey: "actions", title: "操作", width: 130 },
];

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

function emptyForm(): PluginForm {
  return {
    name: "",
    version: "",
    installed_at: today(),
    installation_method: "",
    extra_install_metadata: "{}",
  };
}

function assignForm(value: PluginForm) {
  Object.assign(form, value);
}

function formatMetadata(metadata: unknown): string {
  return JSON.stringify(metadata);
}

async function loadPlugins() {
  loading.value = true;
  try {
    plugins.value = await pluginsApi.list();
  } catch (error) {
    window.alert(`加载插件列表失败：${String(error)}`);
  } finally {
    loading.value = false;
  }
}

function openCreateDialog() {
  editingName.value = null;
  assignForm(emptyForm());
  dialogVisible.value = true;
}

function openEditDialog(plugin: PluginRecord) {
  editingName.value = plugin.name;
  assignForm({ ...plugin, extra_install_metadata: JSON.stringify(plugin.extra_install_metadata, null, 2) });
  dialogVisible.value = true;
}

async function savePlugin() {
  let extraInstallMetadata: unknown;
  try {
    extraInstallMetadata = JSON.parse(form.extra_install_metadata);
  } catch {
    window.alert("额外安装信息元数据必须是有效 JSON。");
    return;
  }

  const plugin: PluginRecord = {
    name: form.name.trim(),
    version: form.version.trim(),
    installed_at: form.installed_at,
    installation_method: form.installation_method.trim(),
    extra_install_metadata: extraInstallMetadata,
  };
  if (!plugin.name || !plugin.version || !plugin.installed_at || !plugin.installation_method) {
    window.alert("请填写所有必填字段。");
    return;
  }

  saving.value = true;
  try {
    if (editingName.value) {
      await pluginsApi.update(editingName.value, plugin);
    } else {
      await pluginsApi.create(plugin);
    }
    dialogVisible.value = false;
    await loadPlugins();
  } catch (error) {
    window.alert(`保存插件失败：${String(error)}`);
  } finally {
    saving.value = false;
  }
}

async function removePlugin(name: string) {
  try {
    await pluginsApi.remove(name);
    await loadPlugins();
  } catch (error) {
    window.alert(`删除插件失败：${String(error)}`);
  }
}

onMounted(loadPlugins);
</script>

<style scoped lang="scss">
.metadata-preview {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
