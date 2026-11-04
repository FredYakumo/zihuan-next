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
                {{ stageHeadText(entry.stage, entry.id) }}
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
                  v-else-if="llmView(entry.id)"
                  class="context-llm"
                  :class="`context-llm--${llmView(entry.id)?.status}`"
                >
                  <div class="context-llm-status">
                    <t-loading
                      v-if="llmView(entry.id)?.status === 'running'"
                      size="12px"
                    />
                    <span>{{ llmView(entry.id)?.statusText }}</span>
                  </div>
                  <dl class="context-llm-grid">
                    <div
                      v-for="field in llmView(entry.id)?.fields"
                      :key="field.label"
                      class="context-llm-field"
                    >
                      <dt>{{ field.label }}</dt>
                      <dd>{{ field.value }}</dd>
                    </div>
                  </dl>
                  <!-- The raw request/response messages of this inference sit above the rendered
                       output, so reading starts from what the model was actually sent. -->
                  <div
                    v-if="llmMessageList(entry.id).length"
                    class="context-llm-section"
                  >
                    <button
                      type="button"
                      class="context-llm-toggle"
                      @click="toggleLlmMessages(entry.id)"
                    >
                      <span class="context-llm-toggle-icon">
                        {{ isLlmMessagesOpen(entry.id) ? "▾" : "▸" }}
                      </span>
                      LLMMessage List（{{ llmMessageList(entry.id).length }} 条）
                    </button>
                    <div v-if="isLlmMessagesOpen(entry.id)" class="context-stage-raw">
                      <div
                        v-for="(item, messageIndex) in llmMessageList(entry.id)"
                        :key="messageIndex"
                        class="context-stage-raw-item"
                        :class="{ 'context-stage-raw-item--output': item.output }"
                      >
                        <div class="context-stage-raw-head">
                          <span class="context-stage-raw-role" :data-role="item.message.role || undefined">{{ item.message.role }}</span>
                          <span v-if="item.output" class="context-stage-raw-output-label">输出</span>
                          <span class="context-stage-raw-index">#{{ messageIndex + 1 }}</span>
                        </div>
                        <div
                          v-if="item.message.reasoning"
                          class="context-thinking"
                          :class="{ collapsed: !isItemThinkingOpen(entry.id, messageIndex) }"
                        >
                          <button
                            type="button"
                            class="context-thinking-toggle"
                            @click="toggleItemThinking(entry.id, messageIndex)"
                          >
                            <span class="context-thinking-icon">
                              {{ isItemThinkingOpen(entry.id, messageIndex) ? "▾" : "▸" }}
                            </span>
                            思考过程
                          </button>
                          <pre
                            v-if="isItemThinkingOpen(entry.id, messageIndex)"
                            class="context-thinking-content"
                          >{{ item.message.reasoning }}</pre>
                        </div>
                        <pre class="context-stage-raw-content">{{ item.message.content }}</pre>
                        <template v-if="item.message.toolCalls?.length">
                          <div class="context-stage-raw-tool-label">工具调用</div>
                          <JsonView :source="item.message.toolCalls" />
                        </template>
                        <div
                          v-if="item.message.toolCallId"
                          class="context-stage-raw-tool-id mono"
                        >tool_call_id: {{ item.message.toolCallId }}</div>
                      </div>
                    </div>
                  </div>
                  <template v-if="llmView(entry.id)?.content || llmView(entry.id)?.reasoning">
                    <div
                      v-if="llmView(entry.id)?.reasoning"
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
                      >{{ llmView(entry.id)?.reasoning }}</pre>
                    </div>
                    <pre
                      v-if="llmView(entry.id)?.content"
                      class="context-llm-stream"
                    >{{ llmView(entry.id)?.content }}</pre>
                  </template>
                  <!-- The tool calls this inference's output requested, in request order: the
                       loop reads as request → tools → results, the next entry carries them as
                       its input messages. -->
                  <div
                    v-if="llmToolsFor(entry.id).length"
                    class="context-llm-section"
                  >
                    <div class="context-llm-tools-label">
                      工具调用（{{ llmToolsFor(entry.id).length }}）
                    </div>
                    <div
                      v-for="tool in llmToolsFor(entry.id)"
                      :key="tool.id"
                      class="context-llm-tool"
                      :data-memory-tone="isToolRawView(tool.id) ? undefined : memoryToolAppearance(tool.name, tool.arguments)?.tone"
                    >
                      <div class="context-llm-tool-head">
                        <span class="context-tool-title">工具调用 · {{ tool.name }}
                          <span
                            v-if="!isToolRawView(tool.id) && memoryToolAppearance(tool.name, tool.arguments)"
                            class="context-memory-tool-label"
                          >{{ memoryToolAppearance(tool.name, tool.arguments)?.label }}</span>
                        </span>
                        <span class="context-tool-status">
                          <t-loading v-if="tool.running" size="12px" />
                          <span v-else-if="tool.durationMs !== undefined" class="muted">{{ tool.durationMs }} ms</span>
                        </span>
                      </div>
                      <JsonView :source="tool.arguments" />
                      <!-- A sub-agent this call started renders in place, below the call. -->
                      <ContextSubAgentBox
                        v-for="node in isToolRawView(tool.id) ? [] : subAgentsFor(tool.callId)"
                        :key="node.key"
                        :node="node"
                        :running="tool.running"
                      />
                      <template v-if="tool.result !== undefined">
                        <div
                          class="context-tool-result-label"
                          :class="{ 'context-tool-result-label--failed': toolResultFailed(tool.result) }"
                        >
                          {{ toolResultFailed(tool.result) ? "错误" : "结果" }}
                        </div>
                        <div
                          class="context-tool-result"
                          :class="{ 'context-tool-result--failed': toolResultFailed(tool.result) }"
                        >
                          <JsonView :source="tool.result" />
                        </div>
                      </template>
                      <button
                        v-if="isSpecialToolCall(tool)"
                        type="button"
                        class="context-view-toggle"
                        @click="toggleToolRawView(tool.id)"
                      >
                        {{ isToolRawView(tool.id) ? "rich视图" : "原始视图" }}
                      </button>
                    </div>
                  </div>
                  <div v-if="llmView(entry.id)?.error" class="context-llm-error">
                    {{ llmView(entry.id)?.error }}
                  </div>
                </div>
                <div
                  v-else-if="sentMessageBubblesView(entry)"
                  class="context-sent-messages"
                >
                  <div
                    v-for="bubble in sentMessageBubblesView(entry)"
                    :key="bubble.index"
                    class="context-sent-message"
                  >
                    <div class="context-sent-message-body">
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
                      <span class="context-stage-history-role" :data-role="item.role || undefined">{{ item.role }}</span>
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
                      <span class="context-stage-raw-role" :data-role="message.role || undefined">{{ message.role }}</span>
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
                      :data-memory-tone="memoryToolAppearance(tool.name)?.tone"
                    >
                      <div class="context-stage-tool-name mono">{{ tool.name }}</div>
                      <span v-if="memoryToolAppearance(tool.name)" class="context-memory-tool-label">{{ memoryToolAppearance(tool.name)?.label }}</span>
                      <p
                        v-if="tool.description"
                        class="context-stage-tool-description"
                      >{{ tool.description }}</p>
                      <JsonView :source="tool.parameters" />
                    </div>
                  </div>
                </div>
                <JsonView
                  v-if="!stageView(entry.id) && !sentMessageBubblesView(entry)"
                  :source="entry.detail"
                />
              </template>
            </div>
          </div>
          <template v-else>
            <div class="context-bubble-row context-bubble-row--tool">
              <div class="context-bubble context-bubble--tool" :data-memory-tone="memoryToolAppearance(entry.name, entry.arguments)?.tone">
                <div class="context-bubble-head">
                  <span class="context-tool-title">工具调用 · {{ entry.name }} <span v-if="memoryToolAppearance(entry.name, entry.arguments)" class="context-memory-tool-label">{{ memoryToolAppearance(entry.name, entry.arguments)?.label }}</span></span>
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
      <!-- Floating flag row for the turn being read: its input and time, the record deletion
           control, and the phase badge at the same top-right spot. -->
      <div v-if="isFocusedTurn" class="context-turn-flag" :style="badgeStyle">
        <span class="context-turn-flag-text" :title="turn.userText">{{ turn.userText || "（无文本输入）" }}</span>
        <span class="context-turn-flag-time">{{ formatTime(turn.createdAt) }}</span>
        <button
          type="button"
          class="context-turn-flag-delete"
          :disabled="!turn.finished || deleting"
          :title="!turn.finished ? '回复生成中，结束后才能删除' : '删除本次对话记录'"
          @click="emit('delete-turn')"
        >{{ deleting ? "删除中…" : "删除记录" }}</button>
        <div
          v-if="activePhase"
          class="context-phase-badge"
          :style="{ '--phase-color': activePhase.color }"
        >
          {{ activePhase.name }}
        </div>
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
import type { ContextToolItem, ContextTurnView } from "./useChatContext";
import { toolResultFailed } from "./useChatContext";
import {
  RATE_LIMIT_STAGE,
  isLlmStage,
  memoryToolAppearance,
  rateLimitSeverity,
  rateLimitText,
  sentMessageBubbles,
  useChatContextTurn,
  type StageRawMessage,
} from "./ChatContextTurn";
import JsonView from "./JsonView.vue";
import ContextSubAgentBox from "./ContextSubAgentBox.vue";

const props = defineProps<{ turn: ContextTurnView; deleting?: boolean }>();

const emit = defineEmits<{ "delete-turn": [] }>();

/** Bubble head text: rate limit gets a fixed label, LLM inferences are numbered per phase. */
function stageHeadText(stage: string, id: string): string {
  if (stage === RATE_LIMIT_STAGE) return "限流";
  if (isLlmStage(stage)) return `llm model inference · 第 ${llmIndexFor(id)} 次`;
  return `环节 · ${stage}`;
}

function rateLimitClass(stage: string, detail: unknown): string {
  const severity = rateLimitSeverity(stage, detail);
  return severity ? `context-bubble--limit-${severity}` : "";
}

/** Outgoing messages a stage reports, rendered as message bubbles inside the stage card. */
function sentMessageBubblesView(entry: { detail: unknown }) {
  return sentMessageBubbles(entry.detail);
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
  isLlmMessagesOpen,
  toggleLlmMessages,
  isItemThinkingOpen,
  toggleItemThinking,
  isStageToolsOpen,
  toggleStageTools,
  isStageRawView,
  toggleStageRawView,
  isToolRawView,
  toggleToolRawView,
  llmToolsFor,
  llmIndexFor,
  phases,
  subAgentsFor,
  orphanSubAgents,
  activePhase,
  badgeStyle,
  isFocusedTurn,
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

function llmView(id: string) {
  return stageView(id)?.llm;
}

/**
 * The messages of one inference in order: everything the model was sent, then the message it
 * returned. The response carries the tool calls of a loop iteration and the settled reply of
 * the last one, which no later request would otherwise show. A response with only thinking
 * still lists, showing its 思考过程 inside the item.
 */
function llmMessageList(id: string) {
  const view = llmView(id);
  if (!view) return [];
  const messages: Array<{ message: StageRawMessage; output: boolean }> = view.messages.map(
    (message) => ({ message, output: false }),
  );
  const output = view.output;
  if (output && (output.content.trim() || output.toolCalls?.length || output.reasoning)) {
    messages.push({ message: output, output: true });
  }
  return messages;
}

/** A call whose rich display replaces the plain payload: memory tools and sub-agent calls. */
function isSpecialToolCall(tool: ContextToolItem): boolean {
  return (
    Boolean(memoryToolAppearance(tool.name, tool.arguments)) ||
    subAgentsFor(tool.callId).length > 0
  );
}
</script>

<style scoped lang="scss">
@use "./ChatContextTurn" as *;
</style>
