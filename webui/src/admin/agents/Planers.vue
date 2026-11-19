<template>
    <section class="page planer-page">
        <AdminPageHeader title="Planer 配置">
            <t-button theme="primary" @click="create()">新建 Planer</t-button>
        </AdminPageHeader>
        <t-alert v-if="error" theme="error" :message="error" closable class="planer-alert" @close="error = ''" />
        <t-card bordered>
            <t-table :data="items" :columns="columns" row-key="config_id" :loading="loading">
                <template #name="{ row }">
                    <div class="planer-name-cell">
                        <span>{{ row.name }}</span>
                        <t-tag size="small" variant="light" :theme="row.enabled ? 'primary' : 'default'">{{ row.enabled ? '已启用' : '已禁用' }}</t-tag>
                    </div>
                </template>
                <template #model="{ row }">{{ modelName(row.llm_ref_id) }}</template>
                <template #members="{ row }">
                    <t-space break-line size="4px">
                        <t-tag v-for="member in row.members" :key="member.role_id" size="small" variant="light">{{ roleLabel(member.role_id) }}</t-tag>
                    </t-space>
                </template>
                <template #status="{ row }">
                    <t-tag size="small" variant="light" :theme="row.runtime?.status === 'running' ? 'success' : 'default'">{{
                        row.runtime?.status === 'running' ? '运行中' : '已停止' }}</t-tag>
                </template>
                <template #actions="{ row }">
                    <t-button variant="text" size="small" @click="lifecycle(row)">{{ row.runtime?.status === 'running' ? '停止' : '启动' }}</t-button>
                    <t-button variant="text" size="small" :disabled="row.runtime?.status !== 'running'" @click="chatAgent = planerChatTarget(row)">对话</t-button>
                    <t-button variant="text" size="small" @click="edit(row)">编辑</t-button>
                    <t-popconfirm content="删除此 Planer？" @confirm="remove(row)">
                        <t-button theme="danger" variant="text" size="small" :disabled="row.runtime?.status === 'running'">删除</t-button>
                    </t-popconfirm>
                </template>
                <template #empty><div class="planer-empty">暂无 Planer</div></template>
            </t-table>
        </t-card>
        <AgentChatDialog :agent="chatAgent" @close="chatAgent = null" />
    </section>
</template>
<script setup lang="ts">
import { planerChatTarget } from "../../api/planers";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import AgentChatDialog from "../components/AgentChatDialog.vue";
import { usePlaners } from "./Planers";
const { items, loading, error, chatAgent, roleLabel, modelName, create, edit, lifecycle, remove } = usePlaners();

const columns = [
    { colKey: "name", title: "名称", width: 240 },
    { colKey: "model", title: "模型", width: 200 },
    { colKey: "members", title: "Role", minWidth: 240 },
    { colKey: "status", title: "状态", width: 100 },
    { colKey: "actions", title: "操作", width: 240, fixed: "right" as const },
];
</script>
<style scoped lang="scss" src="./Planers.scss"></style>
