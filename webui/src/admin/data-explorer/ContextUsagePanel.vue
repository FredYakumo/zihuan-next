<template>
  <aside class="context-usage-panel" :class="[`context-usage-panel--${tone}`, { 'context-usage-panel--collapsed': collapsed }]">
    <button
      v-if="collapsed"
      type="button"
      class="context-usage-orb"
      :style="{ '--usage': `${usedPercent}%` }"
      :title="`当前上下文 ${usedLabel} / ${limitLabel}（${percentLabel}），点击展开`"
      @click="collapsed = false"
    >
      <span class="context-usage-orb-label">{{ percentLabel }}</span>
    </button>
    <template v-else>
      <div class="context-usage-head">
        <span class="context-usage-title">当前上下文</span>
        <span class="context-usage-badge">{{ percentLabel }}</span>
        <button type="button" class="context-usage-collapse" title="收起悬浮窗" @click="collapsed = true">▴</button>
      </div>
      <div class="context-usage-bar"><span :style="{ width: `${usedPercent}%` }"></span></div>
      <div class="context-usage-tokens">
        <strong>{{ usedLabel }}</strong>
        <span class="context-usage-tokens-limit">/ {{ limitLabel }}</span>
      </div>
      <dl class="context-usage-meta">
        <div><dt>会话开始</dt><dd>{{ formatTimeOrDash(startedAt) }}</dd></div>
        <div><dt>最后消息</dt><dd>{{ formatTimeOrDash(lastMessageAt) }}</dd></div>
      </dl>
      <t-button
        class="context-usage-clear"
        theme="danger"
        variant="outline"
        size="small"
        block
        :loading="clearing"
        :disabled="busy"
        :title="busy ? '当前有回复正在生成，结束后才能清除上下文' : ''"
        @click="$emit('clear')"
      >{{ busy ? "回复生成中…" : "清除上下文" }}</t-button>
    </template>
  </aside>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";

import { formatTime } from "../model";

const props = defineProps<{
  usedTokens: number;
  /** Context window of the service's primary model; absent when it cannot be resolved. */
  limitTokens?: number;
  /** Oldest turn still held in the live context, i.e. where this context segment started. */
  startedAt?: string;
  lastMessageAt?: string;
  clearing: boolean;
  /** A turn is still generating; clearing then would race the runtime writing it back. */
  busy?: boolean;
}>();

defineEmits<{ clear: [] }>();

const collapsed = ref(false);

const usedPercent = computed(() => {
  if (!props.limitTokens || props.limitTokens < 1) return 0;
  return Math.min((props.usedTokens / props.limitTokens) * 100, 100);
});
const percentLabel = computed(() => props.limitTokens ? `${usedPercent.value.toFixed(1)}%` : "—");
const usedLabel = computed(() => props.usedTokens.toLocaleString());
const limitLabel = computed(() => props.limitTokens ? `${props.limitTokens.toLocaleString()} tokens` : "上限未知");
const tone = computed(() => {
  if (!props.limitTokens) return "normal";
  if (usedPercent.value >= 90) return "danger";
  if (usedPercent.value >= 75) return "warning";
  return "normal";
});

function formatTimeOrDash(value?: string): string {
  return value ? formatTime(value) : "—";
}
</script>

<style scoped lang="scss">
@use "./ContextUsagePanel" as *;
</style>
