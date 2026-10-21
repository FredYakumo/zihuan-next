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
          <t-tag variant="light">{{ serviceTypeLabel(service.role_service_type.type) }}</t-tag>
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

      <template v-else-if="capability === 'contexts'">
        <div v-if="!selectedSenderId" class="context-users">
          <div class="search-form search-form--simple">
            <t-button variant="outline" :loading="loadingUsers" @click="loadUsers">刷新用户列表</t-button>
            <span class="muted">共 {{ users.length }} 位用户与当前角色有运行时上下文{{ backend === "redis" ? "（Redis 后端）" : "（内存后端，重启丢失）" }}</span>
          </div>
          <t-table v-if="users.length" row-key="sender_id" :data="users" :columns="userColumns" :hover="true" @row-click="onUserRowClick">
            <template #sender="{ row }"><div>{{ row.sender_name }}</div><div class="mono muted">{{ row.sender_id }}</div></template>
            <template #group="{ row }">{{ row.group_name || '-' }}</template>
            <template #last="{ row }">{{ row.last_time || '-' }}</template>
          </t-table>
          <div v-else-if="!loadingUsers" class="empty-state">暂无用户运行时上下文。仅显示启用本功能后与该角色对话过的用户。</div>
        </div>
        <div v-else class="context-detail">
          <div class="context-detail-head">
            <t-button variant="outline" @click="clearSelection">返回用户列表</t-button>
            <strong>{{ selectedTitle }}</strong>
            <span class="mono muted">{{ selectedSenderId }}</span>
          </div>
          <ChatContextFlow
            :backend="backend"
            :loading-context="loadingContext"
            :turns="turns"
            :archived-turns="archivedTurns"
            :estimated-context-tokens="estimatedContextTokens"
            :input-tokens="inputTokens"
            :output-tokens="outputTokens"
            :average-tokens-per-second="averageTokensPerSecond"
            :average-cache-hit-rate="averageCacheHitRate"
          />
        </div>
      </template>

      <template v-else>
        <div class="search-form search-form--simple"><t-input v-model="image.nameQuery" placeholder="图片名" clearable @enter="searchImages" /><t-input v-model="image.descriptionQuery" placeholder="图片描述" clearable @enter="searchImages" /><t-input-number v-model="image.limit" :min="1" :max="50" /><t-button theme="primary" :loading="image.loading" @click="searchImages">搜索</t-button></div>
        <div v-if="image.items.length" class="image-grid"><article v-for="item in image.items" :key="item.object_id" class="image-item"><img v-if="item.url && !failedPreviews.has(item.object_id)" :src="item.url" :alt="item.name || item.media_id || '图片'" referrerpolicy="no-referrer" loading="lazy" @error="onImageError(item.object_id)" /><div v-else class="image-placeholder">无预览</div><div class="image-info"><strong>{{ item.name || item.media_id || '未命名图片' }}</strong><p>{{ item.description || '未提供图片描述' }}</p><div><t-tag v-for="kind in item.match_kinds" :key="kind" size="small" variant="light" :theme="kind === 'keyword' ? 'primary' : 'default'">{{ matchLabel(kind) }}</t-tag></div><span class="muted">{{ item.source || item.backend }}</span></div></article></div>
        <div v-else-if="image.searched && !image.loading" class="empty-state">无匹配图片。</div>
      </template>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import { useDataExplorerDetailPage } from "./DataExplorerDetail";
import ChatContextFlow from "./ChatContextFlow.vue";

const {
  capability,
  service,
  loadingService,
  unavailable,
  message,
  messageColumns,
  searchMessages,
  onMessagePaginationChange,
  memory,
  searchMemories,
  mutateMemory,
  image,
  searchImages,
  failedPreviews,
  onImageError,
  pageTitle,
  userColumns,
  onUserRowClick,
  serviceTypeLabel,
  matchLabel,
  returnToList,
  backend,
  loadingUsers,
  users,
  selectedSenderId,
  selectedTitle,
  loadingContext,
  turns,
  archivedTurns,
  estimatedContextTokens,
  inputTokens,
  outputTokens,
  averageTokensPerSecond,
  averageCacheHitRate,
  loadUsers,
  clearSelection,
} = useDataExplorerDetailPage();
</script>

<style scoped lang="scss">
@use "./DataExplorerDetail" as *;
</style>
