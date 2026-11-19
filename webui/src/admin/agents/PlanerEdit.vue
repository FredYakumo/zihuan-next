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
                    <t-form-item label="推理模型"><t-select v-model="form.llm_ref_id" filterable><t-option
                                v-for="model in models.filter(isChatModelRef)" :key="model.config_id"
                                :value="model.config_id" :label="model.name" /></t-select></t-form-item>
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
    </section>
</template>
<script setup lang="ts">
import { computed } from "vue";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import AvatarEditor from "../components/AvatarEditor.vue";
import { usePlanerEdit } from "./PlanerEdit";
const props = defineProps<{ planerId?: string }>();
const { items, roles, models, loading, saving, error, form, isChatModelRef, roleLabel, isWorkspace, otherOwner, selectMembers, setAvatar, save, backToList } = usePlanerEdit(props.planerId);
const breadcrumbs = computed(() => [
  { label: "Planer 配置", to: "/planers" },
  { label: form.value.config_id ? "编辑 Planer" : "新建 Planer" },
]);
</script>
<style scoped lang="scss" src="./Planers.scss"></style>
