<template>
  <div ref="rootEl" class="context-flow">
    <t-loading v-if="loadingContext" size="small" text="加载上下文中…" class="context-loading" />
    <div v-else-if="allTurns.length === 0" class="empty-state">该用户暂无对话上下文记录。</div>
    <template v-else>
      <template v-for="item in visibleItems" :key="item.turn.turnId">
        <div v-if="item.showArchiveLabel" class="context-archive-label">历史消息</div>
        <div v-else-if="item.showArchiveDivider" class="context-archive-divider" role="separator">
          <span>──── 历史消息 ────</span>
        </div>
        
        <div v-if="item.showHeader" class="context-turn-header" role="separator" :data-turn-id="item.turn.turnId">
          <span class="context-turn-header-title" :title="item.turn.userText">{{ item.turn.userText || "（无文本输入）" }}</span>
          <span class="context-turn-header-time">{{ formatTime(item.turn.createdAt) }}</span>
          <span>{{ item.turn.isGroup ? `群聊 · ${item.turn.groupName || item.turn.groupId || '未知群'}` : '私聊' }}</span>
        </div>
        <ChatContextTurn
          :turn="item.turn"
          :class="{ 'context-turn--flash': item.turn.turnId === flashTurnId }"
          :data-turn-id="item.turn.turnId"
          :deleting="deletingTurnId === item.turn.turnId"
          @delete-turn="emit('delete-turn', item.turn)"
        />
        <t-loading v-if="item.turn.entriesLoading" size="small" text="加载本轮流程…" />
        <div v-if="item.turn.entriesError" role="alert">
          {{ item.turn.entriesError }}
          <t-button variant="text" @click="loadPersistedTurn(item.turn, true)">重试加载流程</t-button>
        </div>
      </template>
    </template>

    <div v-if="!loadingContext" class="context-stats">
      <t-tag variant="light" :theme="backend === 'redis' ? 'success' : 'warning'">
        {{ backend === "redis" ? "Redis" : "In Memory" }}
      </t-tag>
      <div class="stat"><span class="stat-label">上下文消耗</span><span class="stat-value">{{ estimatedContextTokens.toLocaleString() }} tokens</span></div>
      <div class="stat"><span class="stat-label">总输入</span><span class="stat-value">{{ inputTokens.toLocaleString() }}</span></div>
      <div class="stat"><span class="stat-label">总输出</span><span class="stat-value">{{ outputTokens.toLocaleString() }}</span></div>
      <div class="stat"><span class="stat-label">平均 Tokens/s</span><span class="stat-value">{{ formatRate(averageTokensPerSecond) }}</span></div>
      <div class="stat"><span class="stat-label">平均缓存命中率</span><span class="stat-value">{{ formatPercent(averageCacheHitRate) }}</span></div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { formatTime } from "../model";
import {
  formatPercent,
  formatRate,
  type ChatContextFlowEmits,
  type ChatContextFlowExposed,
  type ChatContextFlowProps,
  useChatContextFlow,
} from "./ChatContextFlow";
import ChatContextTurn from "./ChatContextTurn.vue";

const props = defineProps<ChatContextFlowProps>();

const emit = defineEmits<ChatContextFlowEmits>();

const { rootEl, allTurns, visibleItems, flashTurnId, scrollToTurn, loadPersistedTurn } = useChatContextFlow(props);

defineExpose({ scrollToTurn } satisfies ChatContextFlowExposed);
</script>

<style scoped lang="scss">
@use "./ChatContextFlow" as *;
</style>
