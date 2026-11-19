<template>
    <section class="page planer-page">
        <div class="planer-header">
            <h2>Planer 配置</h2><t-button @click="create()">新建 Planer</t-button>
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
        <AgentChatDialog :agent="chatAgent" @close="chatAgent = null" />
    </section>
</template>
<script setup lang="ts">
import { planerChatTarget } from "../../api/planers";
import AgentChatDialog from "../components/AgentChatDialog.vue";
import { usePlaners } from "./Planers";
const { items, roles, models, loading, error, chatAgent, roleLabel, modelName, create, edit, lifecycle, remove } = usePlaners();
</script>
<style scoped lang="scss" src="./Planers.scss"></style>
