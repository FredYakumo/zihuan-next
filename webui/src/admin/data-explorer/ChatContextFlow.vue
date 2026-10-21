<template>
  <div class="context-flow">
    <t-loading v-if="loadingContext" size="small" text="加载上下文中…" class="context-loading" />
    <div v-else-if="turns.length === 0 && archivedTurns.length === 0" class="empty-state">该用户暂无对话上下文记录。</div>
    <template v-else>
      <div v-if="archivedTurns.length" class="context-archive">
        <div class="context-archive-label">旧的历史</div>
        <template v-for="(turn, index) in archivedTurns" :key="`archived:${turn.turnId}`">
          <div v-if="index > 0 && !turn.continuesPrevious" class="context-turn-divider" role="separator"></div>
          <ChatContextTurn :turn="turn" />
        </template>
        <div class="context-archive-divider" role="separator">
          <span>-----旧的历史-----</span>
        </div>
      </div>

      <template v-for="(turn, index) in turns" :key="turn.turnId">
        <div v-if="index > 0 && !turn.continuesPrevious" class="context-turn-divider" role="separator"></div>
        <ChatContextTurn :turn="turn" />
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
import type { ContextBackend, ContextTurnView } from "./useChatContext";
import { formatPercent, formatRate } from "./ChatContextFlow";
import ChatContextTurn from "./ChatContextTurn.vue";

defineProps<{
  backend: ContextBackend;
  loadingContext: boolean;
  turns: ContextTurnView[];
  archivedTurns: ContextTurnView[];
  estimatedContextTokens: number;
  inputTokens: number;
  outputTokens: number;
  averageTokensPerSecond?: number;
  averageCacheHitRate?: number;
}>();
</script>

<style scoped lang="scss">
@use "./ChatContextFlow" as *;
</style>
