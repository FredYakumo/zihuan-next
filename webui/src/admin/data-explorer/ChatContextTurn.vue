<template>
  <section ref="rootEl" class="context-turn">
    <div class="context-bubble-row context-bubble-row--user">
      <div class="context-bubble context-bubble--user">
        <div class="context-bubble-head">用户输入</div>
        <p v-if="turn.groupName" class="context-bubble-group">{{ turn.groupName }}</p>
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
                <button
                  v-if="stageView(entry.id)?.rawMessages?.length"
                  type="button"
                  class="context-stage-view-toggle"
                  @click="toggleStageRawView(entry.id)"
                >
                  {{ isStageRawView(entry.id) ? "可读视图" : "原始视图" }}
                </button>
                <span class="context-bubble-time">{{ formatTime(entry.at) }}</span>
              </div>
              <div v-if="rateLimitText(entry.stage, entry.detail)" class="context-rate-limit">
                {{ rateLimitText(entry.stage, entry.detail) }}
              </div>
              <template v-else>
                <div
                  v-if="embeddingView(entry.id)"
                  class="context-embedding"
                  :class="`context-embedding--${embeddingView(entry.id)?.status}`"
                >
                  <div class="context-embedding-status">
                    <t-loading
                      v-if="embeddingView(entry.id)?.status === 'running'"
                      size="12px"
                    />
                    <span>{{ embeddingView(entry.id)?.statusText }}</span>
                  </div>
                  <dl class="context-embedding-grid">
                    <div
                      v-for="field in embeddingView(entry.id)?.fields"
                      :key="field.label"
                      class="context-embedding-field"
                    >
                      <dt>{{ field.label }}</dt>
                      <dd>{{ field.value }}</dd>
                    </div>
                  </dl>
                  <div
                    v-if="embeddingView(entry.id)?.inputs.length"
                    class="context-embedding-section"
                  >
                    <button
                      type="button"
                      class="context-embedding-toggle"
                      @click="toggleEmbeddingSection(entry.id, 'inputs')"
                    >
                      <span class="context-embedding-toggle-icon">
                        {{ isEmbeddingSectionOpen(entry.id, "inputs") ? "▾" : "▸" }}
                      </span>
                      输入（{{ embeddingView(entry.id)?.inputs.length }}）
                    </button>
                    <ol
                      v-if="isEmbeddingSectionOpen(entry.id, 'inputs')"
                      class="context-embedding-list"
                    >
                      <li
                        v-for="input in embeddingView(entry.id)?.inputs"
                        :key="input.index"
                        class="context-embedding-input"
                      >
                        <span class="context-embedding-input-head">
                          <span class="mono">#{{ input.index }}</span>
                          <span class="context-embedding-input-chars">{{ input.chars }} 字</span>
                        </span>
                        <pre class="context-embedding-input-text">{{ input.text }}</pre>
                      </li>
                    </ol>
                  </div>
                  <div
                    v-if="embeddingView(entry.id)?.vectors.length"
                    class="context-embedding-section"
                  >
                    <button
                      type="button"
                      class="context-embedding-toggle"
                      @click="toggleEmbeddingSection(entry.id, 'vectors')"
                    >
                      <span class="context-embedding-toggle-icon">
                        {{ isEmbeddingSectionOpen(entry.id, "vectors") ? "▾" : "▸" }}
                      </span>
                      输出向量（{{ embeddingView(entry.id)?.vectors.length }}）
                    </button>
                    <ol
                      v-if="isEmbeddingSectionOpen(entry.id, 'vectors')"
                      class="context-embedding-list"
                    >
                      <li
                        v-for="vector in embeddingView(entry.id)?.vectors"
                        :key="vector.index"
                        class="context-embedding-vector"
                      >
                        <span class="context-embedding-vector-head">
                          <span class="mono">#{{ vector.index }}</span>
                          <span class="context-embedding-vector-dims">{{ vector.dimensions }} 维</span>
                        </span>
                        <pre class="context-embedding-vector-values">{{ vector.values }}</pre>
                      </li>
                    </ol>
                  </div>
                  <div
                    v-if="embeddingView(entry.id)?.error"
                    class="context-embedding-error"
                  >
                    {{ embeddingView(entry.id)?.error }}
                  </div>
                </div>
                <div
                  v-else-if="sentBubblesView(entry)"
                  class="context-sent-bubbles"
                >
                  <div
                    v-for="bubble in sentBubblesView(entry)"
                    :key="bubble.index"
                    class="context-sent-bubble"
                  >
                    <div class="context-sent-bubble-body">
                      <template
                        v-for="(segment, segmentIndex) in bubble.segments"
                        :key="segmentIndex"
                      >
                        <span v-if="segment.type === 'text'" class="context-sent-text">{{ segment.text }}</span>
                        <span
                          v-else-if="segment.type === 'at'"
                          class="context-sent-at"
                        >@{{ segment.target ?? "?" }}</span>
                        <span
                          v-else-if="segment.type === 'reply'"
                          class="context-sent-reply"
                        >引用消息 {{ segment.id }}</span>
                        <span
                          v-else-if="segment.type === 'image'"
                          class="context-sent-image"
                        >[图片{{ segment.source ? ` · ${segment.source}` : "" }}]</span>
                        <ul
                          v-else-if="segment.type === 'forward'"
                          class="context-sent-forward"
                        >
                          <li
                            v-for="(node, nodeIndex) in segment.nodes ?? []"
                            :key="nodeIndex"
                          >
                            {{ node }}
                          </li>
                        </ul>
                      </template>
                    </div>
                    <p v-if="bubble.delay_ms > 0" class="context-sent-delay">
                      延迟: {{ bubble.delay_ms }} ms
                    </p>
                  </div>
                </div>
                <pre
                  v-else-if="stageView(entry.id)?.text && !isStageRawView(entry.id)"
                  class="context-system-prompt"
                >{{ stageView(entry.id)?.text }}</pre>
                <div v-if="stageView(entry.id)?.meta" class="context-stage-meta">
                  {{ stageView(entry.id)?.meta }}
                </div>
                <div
                  v-if="stageView(entry.id)?.reasoning"
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
                  >{{ stageView(entry.id)?.reasoning }}</pre>
                </div>
                <div
                  v-if="stageView(entry.id)?.history?.length && !isStageRawView(entry.id)"
                  class="context-stage-history"
                >
                  <div class="context-stage-history-label">
                    随本轮发送的历史消息（{{ stageView(entry.id)?.history?.length }} 条）
                  </div>
                  <div
                    v-for="(item, historyIndex) in stageView(entry.id)?.history"
                    :key="historyIndex"
                    class="context-stage-history-item"
                  >
                    <div class="context-stage-history-head">
                      <span class="context-stage-history-role">{{ item.role }}</span>
                      <span class="context-stage-history-chars">{{ item.chars }} 字</span>
                    </div>
                    <pre class="context-stage-history-text">{{ item.text }}</pre>
                  </div>
                </div>
                <div v-if="isStageRawView(entry.id)" class="context-stage-raw">
                  <div
                    v-for="(message, messageIndex) in stageView(entry.id)?.rawMessages"
                    :key="messageIndex"
                    class="context-stage-raw-item"
                  >
                    <div class="context-stage-raw-head">
                      <span class="context-stage-raw-role">{{ message.role }}</span>
                      <span class="context-stage-raw-index">#{{ messageIndex + 1 }}</span>
                    </div>
                    <pre class="context-stage-raw-content">{{ message.content }}</pre>
                    <template v-if="message.toolCalls?.length">
                      <div class="context-stage-raw-tool-label">工具调用</div>
                      <JsonView :source="message.toolCalls" />
                    </template>
                    <div
                      v-if="message.toolCallId"
                      class="context-stage-raw-tool-id mono"
                    >tool_call_id: {{ message.toolCallId }}</div>
                  </div>
                </div>
                <div
                  v-if="stageView(entry.id)?.tools?.length"
                  class="context-stage-tools"
                >
                  <button
                    type="button"
                    class="context-stage-tools-toggle"
                    @click="toggleStageTools(entry.id)"
                  >
                    <span class="context-stage-tools-icon">
                      {{ isStageToolsOpen(entry.id) ? "▾" : "▸" }}
                    </span>
                    工具（{{ stageView(entry.id)?.tools?.length }}）
                  </button>
                  <div
                    v-if="isStageToolsOpen(entry.id)"
                    class="context-stage-tool-list"
                  >
                    <div
                      v-for="tool in stageView(entry.id)?.tools"
                      :key="tool.name"
                      class="context-stage-tool"
                    >
                      <div class="context-stage-tool-name mono">{{ tool.name }}</div>
                      <p
                        v-if="tool.description"
                        class="context-stage-tool-description"
                      >{{ tool.description }}</p>
                      <JsonView :source="tool.parameters" />
                    </div>
                  </div>
                </div>
                <JsonView
                  v-if="!stageView(entry.id) && !sentBubblesView(entry)"
                  :source="entry.detail"
                />
              </template>
            </div>
          </div>
          <template v-else>
            <div class="context-bubble-row context-bubble-row--tool">
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
                  <div
                    class="context-tool-result-label"
                    :class="{ 'context-tool-result-label--failed': toolResultFailed(entry.result) }"
                  >
                    {{ toolResultFailed(entry.result) ? "错误" : "结果" }}
                  </div>
                  <div
                    class="context-tool-result"
                    :class="{ 'context-tool-result--failed': toolResultFailed(entry.result) }"
                  >
                    <JsonView :source="entry.result" />
                  </div>
                </template>
              </div>
            </div>
            <!-- The sub-agent's own flow gets its own box below the call that started it; the
                 call box above stays as recorded. -->
            <div
              v-for="node in subAgentsFor(entry.callId)"
              :key="node.key"
              class="context-bubble-row context-bubble-row--sub-agent"
            >
              <ContextSubAgentBox :node="node" :running="entry.running" />
            </div>
          </template>
        </template>
      </div>
    </div>

    <!-- Sub-agent flows whose tool call is missing from the turn still show, one level down. -->
    <div v-if="orphanSubAgents.length" class="context-orphan-sub-agents">
      <ContextSubAgentBox
        v-for="node in orphanSubAgents"
        :key="node.key"
        :node="node"
      />
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
import { toolResultFailed } from "./useChatContext";
import {
  RATE_LIMIT_STAGE,
  rateLimitSeverity,
  rateLimitText,
  sentBubbles,
  useChatContextTurn,
} from "./ChatContextTurn";
import JsonView from "./JsonView.vue";
import ContextSubAgentBox from "./ContextSubAgentBox.vue";

const props = defineProps<{ turn: ContextTurnView }>();

function rateLimitClass(stage: string, detail: unknown): string {
  const severity = rateLimitSeverity(stage, detail);
  return severity ? `context-bubble--limit-${severity}` : "";
}

/** Outgoing bubbles a stage reports, rendered as chat bubbles inside the stage card. */
function sentBubblesView(entry: { detail: unknown }) {
  return sentBubbles(entry.detail);
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
  isEmbeddingSectionOpen,
  toggleEmbeddingSection,
  isStageToolsOpen,
  toggleStageTools,
  isStageRawView,
  toggleStageRawView,
  phases,
  subAgentsFor,
  orphanSubAgents,
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

function stageView(id: string) {
  return stageViews.value.get(id);
}

function embeddingView(id: string) {
  return stageView(id)?.embedding;
}
</script>

<style scoped lang="scss">
@use "./ChatContextTurn" as *;
</style>
