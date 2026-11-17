<template>
    <section class="page planer-page">
        <div class="planer-header">
            <h2>Planer 配置</h2><t-button @click="edit()">新建 Planer</t-button>
        </div>
        <t-alert v-if="error" theme="error" :message="error" closable @close="error = ''" />
        <t-loading :loading="loading">
            <t-card v-for="item in items" :key="item.config_id" :title="item.name" class="planer-card">
                <template #actions><t-space>
                        <t-tag :theme="item.runtime?.status === 'running' ? 'success' : 'default'">{{
                            item.runtime?.status === 'running' ? '运行中' : '已停止' }}</t-tag>
                        <t-button variant="text" @click="lifecycle(item)">{{ item.runtime?.status === 'running' ? '停止' :
                            '启动' }}</t-button>
                        <t-button variant="text" :disabled="item.runtime?.status !== 'running'"
                            @click="chatAgent = planerChatTarget(item)">对话</t-button>
                        <t-button variant="text" @click="edit(item)">编辑</t-button>
                        <t-popconfirm content="删除此 Planer？" @confirm="remove(item)"><t-button theme="danger"
                                variant="text"
                                :disabled="item.runtime?.status === 'running'">删除</t-button></t-popconfirm>
                    </t-space></template>
                <p>{{ modelName(item.llm_ref_id) }} · {{ item.enabled ? '已启用' : '已禁用' }} · {{ item.members.length }} 个
                    Role</p>
                <t-space break-line><t-tag v-for="member in item.members" :key="member.role_id">{{
                        roleLabel(member.role_id)
                        }}</t-tag></t-space>
            </t-card>
            <t-empty v-if="!loading && !items.length" description="暂无 Planer" />
        </t-loading>
        <t-drawer v-model:visible="visible" :header="form.config_id ? '编辑 Planer' : '新建 Planer'" size="700px"
            :confirm-btn="{ content: '保存', loading: saving }" @confirm="save">
            <t-alert v-if="error" theme="error" :message="error" />
            <t-form label-align="top">
                <t-form-item label="名称"><t-input v-model="form.name" /></t-form-item>
                <t-form-item label="头像 URL"><t-input :value="form.avatar_url ?? ''"
                        @change="v => form.avatar_url = String(v) || null" /></t-form-item>
                <t-form-item label="启用"><t-switch v-model="form.enabled" /></t-form-item>
                <t-form-item label="自动启动"><t-switch v-model="form.auto_start" /></t-form-item>
                <t-form-item label="推理模型"><t-select v-model="form.llm_ref_id" filterable><t-option
                            v-for="model in models.filter(isChatModelRef)" :key="model.config_id"
                            :value="model.config_id" :label="model.name" /></t-select></t-form-item>
                <t-form-item label="提示词"><t-textarea v-model="form.prompt"
                        placeholder="描述此 Planer 的目标与调度偏好" /></t-form-item>
                <t-form-item label="所属 Role（可暂不选择）">
                    <t-select :value="form.members.map(m => m.role_id)" multiple filterable @change="selectMembers">
                        <t-option v-for="role in roles" :key="role.config_id" :value="role.config_id"
                            :disabled="!!otherOwner(role.config_id)"
                            :label="`${role.name} · ${role.role_service_type.type} · ${role.runtime.status}${role.enabled ? '' : '（已禁用）'} · ${role.config_id} · ${items.find(p => p.members.some(m => m.role_id === role.config_id))?.name || '未分配'}`" />
                    </t-select>
                </t-form-item>
                <t-card v-for="member in form.members" :key="member.role_id" :title="roleLabel(member.role_id)"
                    class="planer-card">
                    <t-form-item label="职责与委托时机"><t-textarea v-model="member.description" /></t-form-item>
                    <t-form-item v-if="isWorkspace(member.role_id)" label="默认工作目录（绝对路径）"><t-input
                            :value="member.workspace_path ?? ''"
                            @change="v => member.workspace_path = String(v)" /></t-form-item>
                </t-card>
            </t-form>
        </t-drawer>
        <AgentChatDialog :agent="chatAgent" @close="chatAgent = null" />
    </section>
</template>
<script setup lang="ts">
import { planerChatTarget } from "../../api/planers";
import AgentChatDialog from "../components/AgentChatDialog.vue";
import { usePlaners } from "./Planers";
const { items, roles, models, loading, saving, visible, error, chatAgent, form, isChatModelRef, roleLabel, modelName, isWorkspace, otherOwner, edit, selectMembers, save, lifecycle, remove } = usePlaners();
</script>
<style scoped lang="scss" src="./Planers.scss"></style>
