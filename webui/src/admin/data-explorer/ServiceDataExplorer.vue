<template>
    <section class="page data-explorer-page">
        <AdminPageHeader :breadcrumbs="[{ label: '数据检索', to: '/data-explorer' }, { label: 'Service 数据' }]" />
        <t-card title="Service" bordered header-bordered>
            <t-table row-key="config_id" :data="explorerServices" :columns="columns" :hover="true">
                <template #name="{ row }"><strong>{{ row.name }}</strong>
                    <div class="mono service-id">{{ row.config_id }}</div>
                </template>
                <template #type="{ row }"><t-tag variant="light">{{ serviceTypeLabel(row.role_service_type.type)
                        }}</t-tag></template>
                <template #status="{ row }"><t-tag variant="light" :theme="row.enabled ? 'success' : 'default'">{{
                    row.enabled ? row.runtime.status : '已停用' }}</t-tag></template>
                <template #actions="{ row }"><t-button v-for="capability in serviceCapabilities(row)" :key="capability"
                        class="action-button" size="small" variant="outline"
                        @click="openDetail(row.config_id, capability)">{{ capabilityLabel(capability)
                        }}</t-button></template>
            </t-table>
            <div v-if="explorerServices.length === 0" class="empty-state">暂无配置聊天记录、记忆或图片库的 Service。</div>
        </t-card>
    </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useServiceDataExplorer } from "./ServiceDataExplorer";

const { explorerServices, columns, serviceTypeLabel, capabilityLabel, openDetail, serviceCapabilities } = useServiceDataExplorer();
</script>

<style scoped lang="scss">
@use "./ServiceDataExplorer" as *;
</style>
