<template>
  <section ref="rootEl" class="context-turn">
    <div class="context-bubble-row context-bubble-row--user">
      <div class="context-bubble context-bubble--user">
        <div class="context-bubble-head">用户输入</div>
        <div v-if="turn.quoted" class="context-quote" :class="{ collapsed: !quoteOpen }">
          <button
            type="button"
            class="context-quote-head"
            :title="quoteOpen ? '折叠引用消息' : '展开引用消息'"
            @click="toggleQuote"
          >
            <span class="context-quote-label">引用消息</span>
            <span class="context-quote-arrow">{{ quoteOpen ? "▴" : "▾" }}</span>
            <template v-if="quoteOpen">
              <span v-if="quoteSender" class="context-quote-sender">{{ quoteSender }}</span>
              <span class="context-quote-id">message_id: {{ turn.quoted.messageId }}</span>
            </template>
          </button>
          <template v-if="quoteOpen">
            <div class="context-quote-body">{{ turn.quoted.text }}</div>
            <button type="button" class="context-quote-toggle" @click="toggleQuote">折叠</button>
          </template>
        </div>
        <div class="context-bubble-body">
          <template v-if="richView">
            <template v-for="(segment, index) in segments" :key="index">
              <span v-if="segment.type === 'text'" class="context-inline-text">{{ segment.text }}</span>
              <button
                v-else
                type="button"
                class="context-inline-image"
                @mouseenter="openPreview(segment.media)"
                @mouseleave="closePreview"
              >
                <img :src="segment.media.url" :alt="segment.media.name || segment.media.media_id" referrerpolicy="no-referrer" @error="hideImage(segment.media)" />
              </button>
            </template>
          </template>
          <span v-else class="context-inline-text">{{ turn.userText }}</span>
        </div>
        <button type="button" class="context-view-toggle" @click="toggleRichView">
          {{ richView ? "原始视图" : "rich视图" }}
        </button>
      </div>
    </div>

    <div
      v-for="phase in phases"
      :key="phase.key"
      :ref="(el) => registerPhase(phase.key, el)"
      class="context-phase"
      :style="{ '--phase-color': phase.color }"
    >
      <div class="context-phase-name">{{ phase.name }}</div>
      <div class="context-phase-body">
        <template v-for="entry in phase.entries" :key="entry.id">
          <div v-if="entry.kind === 'stage'" class="context-bubble-row context-bubble-row--stage">
            <div
              class="context-bubble context-bubble--stage"
              :class="rateLimitClass(entry.stage, entry.detail)"
            >
              <div class="context-bubble-head">
                {{ entry.stage === RATE_LIMIT_STAGE ? "限流" : `环节 · ${entry.stage}` }}
                <span class="context-bubble-time">{{ formatTime(entry.at) }}</span>
              </div>
              <div v-if="rateLimitText(entry.stage, entry.detail)" class="context-rate-limit">
                {{ rateLimitText(entry.stage, entry.detail) }}
              </div>
              <template v-else>
                <pre
                  v-if="stageViews.get(entry.id)"
                  class="context-system-prompt"
                >{{ stageViews.get(entry.id)?.text }}</pre>
                <div v-if="stageViews.get(entry.id)?.meta" class="context-stage-meta">
                  {{ stageViews.get(entry.id)?.meta }}
                </div>
                <div
                  v-if="stageViews.get(entry.id)?.reasoning"
                  class="context-thinking"
                  :class="{ collapsed: !stageThinkingOpen(entry.id) }"
                >
                  <button
                    type="button"
                    class="context-thinking-toggle"
                    @click="toggleStageThinking(entry.id)"
                  >
                    <span class="context-thinking-icon">
                      {{ stageThinkingOpen(entry.id) ? "▾" : "▸" }}
                    </span>
                    思考过程
                  </button>
                  <pre
                    v-if="stageThinkingOpen(entry.id)"
                    class="context-thinking-content"
                  >{{ stageViews.get(entry.id)?.reasoning }}</pre>
                </div>
                <div
                  v-if="stageViews.get(entry.id)?.history?.length"
                  class="context-stage-history"
                >
                  <div class="context-stage-history-label">
                    随本轮发送的历史消息（{{ stageViews.get(entry.id)?.history?.length }} 条）
                  </div>
                  <div
                    v-for="(item, historyIndex) in stageViews.get(entry.id)?.history"
                    :key="historyIndex"
                    class="context-stage-history-item"
                  >
                    <span class="context-stage-history-role">{{ item.role }}</span>
                    <span class="context-stage-history-preview">{{ item.preview }}</span>
                    <span class="context-stage-history-chars">{{ item.chars }} 字</span>
                  </div>
                </div>
                <JsonView v-if="!stageViews.get(entry.id)" :source="entry.detail" />
              </template>
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
      </div>
    </div>

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
      <div
        v-if="activePhase"
        class="context-phase-badge"
        :style="{ ...badgeStyle, '--phase-color': activePhase.color }"
      >
        <!-- 正在浏览 · {{ activePhase.name }} -->
        {{ activePhase.name }}
      </div>
      <div v-if="previewMedia?.url" class="context-preview-overlay">
        <figure class="context-preview-frame">
          <img :src="previewMedia.url" :alt="previewMedia.name || previewMedia.media_id" referrerpolicy="no-referrer" />
          <figcaption class="context-preview-caption">
            <span class="mono">{{ previewMedia.media_id }}</span>
            <span v-if="previewMedia.name">{{ previewMedia.name }}</span>
            <span v-if="previewMedia.size_bytes">{{ formatBytes(previewMedia.size_bytes) }}</span>
          </figcaption>
        </figure>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import type { ContextTurnView } from "./useChatContext";
import {
  RATE_LIMIT_STAGE,
  rateLimitSeverity,
  rateLimitText,
  useChatContextTurn,
} from "./ChatContextTurn";
import JsonView from "./JsonView.vue";

const props = defineProps<{ turn: ContextTurnView }>();

function rateLimitClass(stage: string, detail: unknown): string {
  const severity = rateLimitSeverity(stage, detail);
  return severity ? `context-bubble--limit-${severity}` : "";
}

const {
  rootEl,
  previewMedia,
  richView,
  toggleRichView,
  thinkingOpen,
  quoteOpen,
  quoteSender,
  toggleQuote,
  stageThinkingOpen,
  toggleStageThinking,
  phases,
  activePhase,
  badgeStyle,
  registerPhase,
  stageViews,
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
