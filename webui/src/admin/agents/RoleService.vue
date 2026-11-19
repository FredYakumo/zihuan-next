<template>
    <section class="page agent-service-page">
        <AdminPageHeader title="Service 管理">
            <t-button variant="outline" @click="openImportPage">导入配置</t-button>
            <t-button theme="primary" @click="openCreatePage">新建 Service</t-button>
        </AdminPageHeader>

        <t-card class="agent-service-card" bordered>
            <div class="agent-service-toolbar">
                <t-input v-model="filters.keyword" clearable placeholder="搜索名称或 Config ID" />
                <t-select v-model="filters.type">
                    <t-option value="all" label="全部 Service 类型" />
                    <t-option value="qq_chat" label="QQ Chat RoleService" />
                    <t-option value="workspace" label="Workspace RoleService" />
                </t-select>
                <t-select v-model="filters.status">
                    <t-option value="all" label="全部运行状态" />
                    <t-option value="running" label="运行中" />
                    <t-option value="stopped" label="已停止" />
                    <t-option value="error" label="异常" />
                </t-select>
                <div class="agent-service-toolbar-actions">
                    <t-button variant="text" :loading="servicesLoading" @click="load">刷新</t-button>
                    <span>共 {{ filteredServices.length }} 条</span>
                </div>
            </div>

            <t-table row-key="config_id" :data="filteredServices" :columns="columns" :loading="servicesLoading"
                :hover="true" :pagination="false" table-layout="fixed">
                <template #name="{ row }">
                    <div class="agent-service-name-cell">
                        <img v-if="agentAvatarUrl(row)" :src="agentAvatarUrl(row)" class="agent-service-avatar"
                            alt="" />
                        <span v-else class="agent-service-avatar agent-service-avatar--fallback">{{
                            agentInitial(row.name) }}</span>
                        <div><strong>{{ row.name }}</strong><small class="mono">{{ compactId(row.config_id) }}</small>
                        </div>
                    </div>
                </template>
                <template #type="{ row }"><t-tag variant="light">{{ serviceTypeLabel(row.role_service_type.type)
                        }}</t-tag></template>
                <template #model="{ row }"><span :title="llmName(row)">{{ llmName(row) }}</span></template>
                <template #runtime="{ row }"><t-tag :theme="runtimeTheme(row.runtime.status)" variant="light">{{
                        runtimeBadgeText(row) }}</t-tag></template>
                <template #enabled="{ row }"><t-tag :theme="row.enabled ? 'success' : 'default'" variant="light">{{
                    row.enabled ? '已启用' : '已停用' }}</t-tag></template>
                <template #updated="{ row }"><span>{{ formatTime(row.runtime.started_at) }}</span></template>
                <template #actions="{ row }">
                    <div class="agent-service-actions">
                        <t-button variant="text" size="small" @click="openEditorPage(row)">编辑</t-button>
                        <t-button variant="text" size="small" @click="openDuplicatePage(row)">复制添加</t-button>
                        <t-button variant="text" size="small" @click="copyServiceConfigItem(row)">{{ serviceCopiedId ===
                            row.config_id ? '已复制' : '复制' }}</t-button>
                        <t-button variant="text" :theme="row.runtime.status === 'running' ? 'warning' : 'primary'"
                            size="small" @click="toggleServiceRuntime(row)">{{ row.runtime.status === 'running' ? '停止' :
                            '启动' }}</t-button>
                        <t-popconfirm content="确认删除这个 Service 吗？" @confirm="removeService(row.config_id)"><t-button
                                variant="text" theme="danger" size="small">删除</t-button></t-popconfirm>
                    </div>
                </template>
                <template #empty>
                    <div class="agent-service-empty">暂无匹配的 Service。</div>
                </template>
            </t-table>
        </t-card>
        <ServiceRestartConfirmDialog :state="restartDialog.state" @close="restartDialog.close()"
            @save-only="restartDialog.saveOnly()" @restart="restartDialog.restartAll()" />
    </section>
</template>

<script setup lang="ts">
import type { ServiceWithRuntime } from "../../api/client";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import ServiceRestartConfirmDialog from "../components/ServiceRestartConfirmDialog.vue";
import { useRoleServicePage } from "./roleServicePage";

const {
    router,
    restartDialog,
    servicesLoading,
    filters,
    filteredServices,
    columns,
    load,
    removeService,
    toggleServiceRuntime,
    copyServiceConfigItem,
    serviceCopiedId,
    agentAvatarUrl,
    agentInitial,
    compactId,
    serviceTypeLabel,
    llmName,
    runtimeBadgeText,
    runtimeTheme,
    formatTime,
} = useRoleServicePage();

function openCreatePage() {
    void router.push("/services/edit");
}

function openImportPage() {
    void router.push("/services/edit?import=1");
}

function openEditorPage(row: ServiceWithRuntime) {
    void router.push({ path: "/services/edit", query: { id: row.config_id } });
}

function openDuplicatePage(row: ServiceWithRuntime) {
    void router.push({ path: "/services/edit", query: { copy: row.config_id } });
}
</script>

<style scoped lang="scss">
@use "./role-service" as *;
</style>
