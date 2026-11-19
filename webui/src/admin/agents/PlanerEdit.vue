<template>
    <section class="page planer-page">
        <AdminPageHeader :breadcrumbs="breadcrumbs">
            <t-space>
                <t-button variant="outline" @click="backToList">返回列表</t-button>
                <t-button :loading="saving" @click="save">保存</t-button>
            </t-space>
        </AdminPageHeader>
        <t-alert v-if="error" theme="error" :message="error" closable @close="error = ''" />
        <t-loading :loading="loading">
            <t-card class="planer-card" :bordered="true">
                <t-form label-align="top">
                    <t-form-item label="头像">
                        <AvatarEditor :avatar-url="form.avatar_url" :name="form.name"
                            @update:avatar-url="setAvatar" />
                    </t-form-item>
                    <t-form-item label="名称"><t-input v-model="form.name" /></t-form-item>
                    <t-form-item label="启用"><t-switch v-model="form.enabled" /></t-form-item>
                    <t-form-item label="自动启动"><t-switch v-model="form.auto_start" /></t-form-item>
                    <t-form-item label="主模型">
                        <CommonSelect v-model="form.llm_ref_id" placeholder="请选择" filterable
                            :options="chatModelOptions" @change="handleModelChange">
                            <t-option class="planer-add-model-option" value="__add_model__" label="新增模型配置">
                                <span class="planer-add-model-option-content"><AddIcon />新增模型配置</span>
                            </t-option>
                        </CommonSelect>
                    </t-form-item>
                    <t-form-item label="提示词"><t-textarea v-model="form.prompt"
                            placeholder="描述此 Planer 的目标与调度偏好" /></t-form-item>
                    <t-form-item label="所属 Role（可暂不选择）">
                        <t-select :value="form.members.map(m => m.role_id)" multiple filterable
                            @change="selectMembers">
                            <t-option v-for="role in roles" :key="role.config_id" :value="role.config_id"
                                :disabled="!!otherOwner(role.config_id)"
                                :label="`${role.name} · ${role.role_service_type.type} · ${role.runtime.status}${role.enabled ? '' : '（已禁用）'} · ${role.config_id} · ${items.find(p => p.members.some(m => m.role_id === role.config_id))?.name || '未分配'}`" />
                        </t-select>
                    </t-form-item>
                </t-form>
            </t-card>
            <t-card v-for="member in form.members" :key="member.role_id" :title="roleLabel(member.role_id)"
                class="planer-card">
                <t-form-item label="职责与委托时机"><t-textarea v-model="member.description" /></t-form-item>
                <t-form-item v-if="isWorkspace(member.role_id)" label="默认工作目录（绝对路径）"><t-input
                        :value="member.workspace_path ?? ''"
                        @change="v => member.workspace_path = String(v)" /></t-form-item>
            </t-card>
        </t-loading>
        <ConfigImportDialog v-model:visible="showModelConfigDialog" title="新增模型配置" create-label="新增模型配置"
            :loading="modelImporting" @create="openModelCreatePage" @clipboard-import="importModelFromClipboard"
            @file-change="handleModelFileChange" />
    </section>
</template>
<script setup lang="ts">
import { computed } from "vue";
import { AddIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import AvatarEditor from "../components/AvatarEditor.vue";
import CommonSelect from "../components/CommonSelect.vue";
import ConfigImportDialog from "../components/ConfigImportDialog.vue";
import { usePlanerEdit } from "./PlanerEdit";
const props = defineProps<{ planerId?: string }>();
const {
    items, roles, chatModelOptions, loading, saving, error, form,
    showModelConfigDialog, modelImporting,
    roleLabel, isWorkspace, otherOwner, selectMembers, setAvatar, save, backToList,
    handleModelChange, openModelCreatePage, importModelFromClipboard, handleModelFileChange,
} = usePlanerEdit(props.planerId);
const breadcrumbs = computed(() => [
  { label: "Planer 配置", to: "/planers" },
  { label: form.value.config_id ? "编辑 Planer" : "新建 Planer" },
]);
</script>
<style scoped lang="scss" src="./Planers.scss"></style>
