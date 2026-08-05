<template>
  <section class="page data-explorer-detail-page">
    <AdminPageHeader :title="pageTitle">
      <!-- <t-button variant="outline" @click="returnToList">返回 Service 列表</t-button> -->
    </AdminPageHeader>

    <t-card v-if="loadingService" bordered><div class="empty-state">加载 Service 中…</div></t-card>
    <t-card v-else-if="unavailable" bordered><div class="empty-state">该 Service 不存在，或未配置此数据检索能力。</div></t-card>
    <t-card v-else-if="service" bordered header-bordered>
      <template #title>
        <div class="service-card-title">
          <t-button variant="outline" @click="returnToList">返回 Service 列表</t-button>
          <span class="service-name">{{ service.name }}</span>
          <t-tag variant="light">{{ serviceTypeLabel(service.agent_type.type) }}</t-tag>
        </div>
      </template>

      <template v-if="capability === 'messages'">
        <t-form class="search-form" layout="inline" @submit.prevent="searchMessages">
          <div class="message-filter-row">
            <t-form-item label="消息 ID"><t-input v-model="message.filters.message_id" clearable /></t-form-item>
            <t-form-item label="发送者"><t-input v-model="message.filters.sender_name" clearable /></t-form-item>
            <t-form-item label="发送者 ID"><t-input v-model="message.filters.sender_id" clearable /></t-form-item>
            <t-form-item label="群 ID"><t-input v-model="message.filters.group_id" clearable /></t-form-item>
          </div>
          <div class="message-content-row">
            <t-form-item class="message-content-field" label="内容">
              <t-textarea v-model="message.filters.content" clearable :autosize="{ minRows: 3, maxRows: 6 }" />
            </t-form-item>
            <t-button theme="primary" :loading="message.loading" @click="searchMessages">搜索</t-button>
          </div>
        </t-form>
        <t-table v-if="message.records.length" row-key="message_id" :data="message.records" :columns="messageColumns">
          <template #sender="{ row }"><div>{{ row.sender_name }}</div><div class="mono muted">{{ row.sender_id }}</div></template>
          <template #group="{ row }">{{ row.group_name || row.group_id || '-' }}</template>
          <template #content="{ row }"><span class="content-cell">{{ row.content }}</span></template>
        </t-table>
        <div v-else-if="message.searched && !message.loading" class="empty-state">无匹配聊天记录。</div>
        <t-pagination v-if="message.total" class="pagination" :total="message.total" :current="message.page" :page-size="message.pageSize" :page-size-options="[20, 50, 100]" show-jumper @change="onMessagePaginationChange" />
      </template>

      <template v-else-if="capability === 'memories'">
        <div class="search-form search-form--simple"><t-input v-model="memory.query" placeholder="搜索记忆标题或内容" clearable @enter="searchMemories" /><t-input-number v-model="memory.limit" :min="1" :max="50" /><t-button theme="primary" :loading="memory.loading" @click="searchMemories">搜索</t-button><t-button v-if="memory.mutable" variant="outline" @click="mutateMemory('create')">新建记忆</t-button></div>
        <div v-if="memory.items.length" class="memory-list"><article v-for="item in memory.items" :key="item.object_id" class="memory-item"><div class="memory-item-header"><strong>{{ item.title }}</strong><span><t-tag v-for="kind in item.match_kinds" :key="kind" size="small" variant="light" :theme="kind === 'keyword' ? 'primary' : 'default'">{{ matchLabel(kind) }}</t-tag></span></div><p>{{ item.value }}</p><div class="muted">更新于 {{ item.updated_at }} · {{ item.backend }}</div><div v-if="item.mutable" class="memory-actions"><t-button variant="text" size="small" @click="mutateMemory('edit', item)">编辑</t-button><t-button variant="text" theme="danger" size="small" @click="mutateMemory('delete', item)">删除</t-button></div></article></div>
        <div v-else-if="memory.searched && !memory.loading" class="empty-state">无匹配记忆。</div>
      </template>

      <template v-else>
        <div class="search-form search-form--simple"><t-input v-model="image.nameQuery" placeholder="图片名" clearable @enter="searchImages" /><t-input v-model="image.descriptionQuery" placeholder="图片描述" clearable @enter="searchImages" /><t-input-number v-model="image.limit" :min="1" :max="50" /><t-button theme="primary" :loading="image.loading" @click="searchImages">搜索</t-button></div>
        <div v-if="image.items.length" class="image-grid"><article v-for="item in image.items" :key="item.object_id" class="image-item"><img v-if="item.url" :src="item.url" :alt="item.name || item.media_id || '图片'" @error="onImageError" /><div v-else class="image-placeholder">无预览</div><div class="image-info"><strong>{{ item.name || item.media_id || '未命名图片' }}</strong><p>{{ item.description || '未提供图片描述' }}</p><div><t-tag v-for="kind in item.match_kinds" :key="kind" size="small" variant="light" :theme="kind === 'keyword' ? 'primary' : 'default'">{{ matchLabel(kind) }}</t-tag></div><span class="muted">{{ item.source || item.backend }}</span></div></article></div>
        <div v-else-if="image.searched && !image.loading" class="empty-state">无匹配图片。</div>
      </template>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { PrimaryTableCol } from "tdesign-vue-next";

import type { MysqlRecord } from "../../api/client";
import { type ServiceCapability, useDataExplorerDetail } from "../composables/useDataExplorer";

const route = useRoute();
const router = useRouter();
const serviceId = String(route.params.serviceId ?? "");
const capability = String(route.params.capability ?? "") as ServiceCapability;
const { service, loadingService, unavailable, message, searchMessages, changeMessagePage, memory, searchMemories, image, searchImages, mutateMemory } = useDataExplorerDetail(serviceId, capability);
const pageTitle = computed(() => capability === "messages" ? "聊天记录" : capability === "memories" ? "记忆" : "图片库");
const messageColumns: PrimaryTableCol<MysqlRecord>[] = [{ colKey: "message_id", title: "消息 ID", width: 160 }, { colKey: "sender", title: "发送者", width: 160 }, { colKey: "send_time", title: "发送时间", width: 180 }, { colKey: "group", title: "群组", width: 150 }, { colKey: "content", title: "内容" }];
function serviceTypeLabel(type: string) { return type === "qq_chat" ? "QQ Chat" : type === "http_stream" ? "HTTP Stream" : "Workspace"; }
function matchLabel(kind: string) { return kind === "keyword" ? "关键词" : kind === "semantic" ? "语义" : "最近更新"; }
function onMessagePaginationChange(info: { current: number; pageSize: number }) { message.value.pageSize = info.pageSize; changeMessagePage(info.current); }
function returnToList() { void router.push("/data-explorer"); }
function onImageError(event: Event) { (event.target as HTMLImageElement).style.display = "none"; }
</script>

<style scoped lang="scss">
.data-explorer-detail-page { display: grid; gap: 16px; }
.service-card-title { display: flex; align-items: center; gap: 12px; }
.service-name { font-size: 20px; }
.search-form { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 12px; margin-bottom: 16px; }
.search-form :deep(.t-form__item) { margin-bottom: 0; }
.message-filter-row, .message-content-row { display: flex; width: 100%; flex-wrap: wrap; gap: 12px; }
.message-content-row { align-items: flex-start; }
.message-content-field { flex: 1 1 560px; }
.message-content-field :deep(.t-textarea) { width: 100%; }
.search-form--simple :deep(.t-input), .search-form--simple :deep(.t-input-number) { width: min(280px, 100%); }
.content-cell { white-space: pre-wrap; word-break: break-word; }
.pagination { margin-top: 16px; }
.memory-list { display: grid; gap: 10px; }
.memory-item, .image-item { border: 1px solid var(--td-component-border); border-radius: 6px; }
.memory-item { padding: 14px 16px; }
.memory-item-header { display: flex; justify-content: space-between; gap: 12px; }
.memory-item p, .image-info p { margin: 8px 0; white-space: pre-wrap; word-break: break-word; }
.memory-actions { margin-top: 6px; }
.image-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 12px; }
.image-item { overflow: hidden; background: var(--td-bg-color-container); }
.image-item img, .image-placeholder { display: block; width: 100%; height: 160px; object-fit: cover; background: var(--td-bg-color-secondarycontainer); }
.image-placeholder { display: grid; place-items: center; color: var(--td-text-color-placeholder); }
.image-info { display: grid; gap: 6px; padding: 12px; }
.image-info p { min-height: 40px; font-size: 13px; }
.empty-state { padding: 48px 16px; text-align: center; color: var(--td-text-color-placeholder); }
@media (max-width: 768px) { .search-form--simple :deep(.t-input), .search-form--simple :deep(.t-input-number) { width: 100%; } }
</style>
