<template>
  <section class="context-turn">
    <div class="context-bubble-row context-bubble-row--user">
      <div class="context-bubble context-bubble--user">
        <div class="context-bubble-head">用户输入</div>
        <div class="context-bubble-body">
          <template v-for="(segment, index) in segments" :key="index">
            <span v-if="segment.type === 'text'" class="context-inline-text">{{ segment.text }}</span>
            <button v-else type="button" class="context-inline-image" @click.stop="openPreview(segment.media)">
              <img :src="segment.media.url" :alt="segment.media.name || segment.media.media_id" referrerpolicy="no-referrer" @error="hideImage(segment.media)" />
              <span class="context-inline-image-tip">
                <span class="mono">{{ segment.media.media_id }}</span>
                <span v-if="segment.media.size_bytes">{{ formatBytes(segment.media.size_bytes) }}</span>
              </span>
            </button>
          </template>
        </div>
      </div>
    </div>

    <template v-for="entry in turn.entries" :key="entry.id">
      <div v-if="entry.kind === 'stage'" class="context-bubble-row context-bubble-row--stage">
        <div class="context-bubble context-bubble--stage">
          <div class="context-bubble-head">阶段 · {{ entry.stage }}<span class="context-bubble-time">{{ formatTime(entry.at) }}</span></div>
          <pre
            v-if="systemPromptText(entry.stage, entry.detail)"
            class="context-system-prompt"
          >{{ systemPromptText(entry.stage, entry.detail) }}</pre>
          <JsonView v-else :source="entry.detail" />
        </div>
      </div>
      <div v-else class="context-bubble-row context-bubble-row--tool">
        <div class="context-bubble context-bubble--tool">
          <div class="context-bubble-head">
            <span class="context-tool-title">工具调用 · {{ entry.name }}</span>
            <span class="context-tool-status">
              <t-loading v-if="entry.running" size="12px" />
              <span v-else-if="entry.durationMs !== undefined" class="muted">{{ entry.durationMs }} ms</span>
            </span>
          </div>
          <JsonView :source="entry.arguments" />
          <template v-if="entry.result !== undefined">
            <div class="context-tool-result-label">结果</div>
            <JsonView :source="entry.result" />
          </template>
        </div>
      </div>
    </template>

    <div class="context-bubble-row context-bubble-row--assistant">
      <div class="context-bubble context-bubble--assistant">
        <div class="context-bubble-head">Assistant</div>
        <div v-if="turn.reasoningText" class="context-thinking" :class="{ collapsed: !thinkingOpen }">
          <button type="button" class="context-thinking-toggle" @click="thinkingOpen = !thinkingOpen">
            <span class="context-thinking-icon">{{ thinkingOpen ? "▾" : "▸" }}</span>
            思考过程
          </button>
          <pre v-if="thinkingOpen" class="context-thinking-content">{{ turn.reasoningText }}</pre>
        </div>
        <div class="context-bubble-body">
          <span v-if="turn.assistantText" class="context-inline-text">{{ turn.assistantText }}</span>
          <span v-if="!turn.finished" class="context-pending"><t-loading size="14px" /> 推理中…</span>
          <span v-else-if="!turn.assistantText" class="muted">本轮未发送回复</span>
        </div>
      </div>
    </div>

    <div class="context-turn-metrics">
      <span class="context-bubble-time">{{ formatTime(turn.createdAt) }}</span>
      <template v-if="turn.usage">
        <span>输入 {{ turn.usage.prompt_tokens ?? "-" }}</span>
        <span>输出 {{ turn.usage.completion_tokens ?? "-" }}</span>
        <span v-if="turn.tokensPerSecond !== undefined">{{ turn.tokensPerSecond.toFixed(1) }} Tokens/s</span>
        <span v-if="cacheHit(turn.usage) !== undefined">{{ formatPercent(cacheHit(turn.usage)) }} 缓存命中</span>
        <span v-if="turn.generationMs !== undefined">生成 {{ turn.generationMs }} ms</span>
      </template>
    </div>

    <Teleport to="body">
      <div v-if="previewUrl" class="context-preview-overlay" @click.self="closePreview">
        <img :src="previewUrl" alt="预览" />
        <button type="button" class="context-preview-close" @click="closePreview">×</button>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import type { ContextTurnView } from "./useChatContext";
import { systemPromptText, useChatContextTurn } from "./ChatContextTurn";
import JsonView from "./JsonView.vue";

const props = defineProps<{ turn: ContextTurnView }>();

const {
  previewUrl,
  thinkingOpen,
  segments,
  cacheHit,
  formatPercent,
  formatBytes,
  formatTime,
  hideImage,
  openPreview,
  closePreview,
} = useChatContextTurn(() => props.turn);
</script>

<style scoped lang="scss">
@use "./ChatContextTurn" as *;
</style>
