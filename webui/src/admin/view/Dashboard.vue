<template>
  <section class="page dashboard-page">
    <div class="page-hero">
      <h2>运行总览</h2>
      <div class="dashboard-stats">
        <div class="stat-item">
          <span class="muted">连接配置</span>
          <strong>{{ stats.connections }}</strong>
        </div>
        <div class="stat-divider"></div>
        <div class="stat-item">
          <span class="muted">模型配置</span>
          <strong>{{ stats.llm }}</strong>
        </div>
        <div class="stat-divider"></div>
        <div class="stat-item">
          <span class="muted">Agent 数量</span>
          <strong>{{ stats.agents }}</strong>
        </div>
      </div>
    </div>

    <div class="dashboard-panels">
      <section class="panel chat-panel">
        <div class="chat-toolbar">
          <div class="chat-agent-picker">
            <div class="chat-agent-picker-title">选择 Agent</div>
            <div class="chat-agent-cards">
              <div v-if="agentsLoading && agents.length === 0" class="chat-agent-loading" aria-live="polite">
                <span class="chat-agent-loading-spinner"></span>
                <span>Agent 加载中...</span>
              </div>
              <template v-else>
                <button
                  v-for="agent in agents.filter(a => chatEligibleAgentTypes.has(a.agent_type.type))"
                  :key="agent.config_id"
                  class="chat-agent-card"
                  :class="{
                    active: selectedAgentId === agent.config_id,
                    inactive: agent.runtime.status !== 'running' || !chatEligibleAgentTypes.has(agent.agent_type.type),
                    unsupported: !chatEligibleAgentTypes.has(agent.agent_type.type),
                  }"
                  :disabled="!chatEligibleAgentTypes.has(agent.agent_type.type)"
                  @click="selectedAgentId = agent.config_id"
                >
                  <img
                    v-if="agentAvatarUrl(agent)"
                    class="chat-agent-card-avatar"
                    :src="agentAvatarUrl(agent)"
                    alt="agent avatar"
                  />
                  <div v-else class="chat-agent-card-avatar chat-agent-card-avatar--fallback">
                    {{ agentInitial(agent.name) }}
                  </div>
                  <div class="chat-agent-card-meta">
                    <strong>{{ agent.name }}</strong>
                    <span>{{ readableAgentType(agent.agent_type.type) }}</span>
                  </div>
                  <span v-if="!chatEligibleAgentTypes.has(agent.agent_type.type)" class="agent-status-badge unsupported-badge">Dashboard 不可用</span>
                  <span v-else-if="agent.runtime.status !== 'running'" class="agent-status-badge">未运行</span>
                </button>
              </template>
            </div>
          </div>
          <button class="btn ghost" @click="reloadSessions">刷新历史</button>
          <button v-if="isWorkspaceAgent" class="btn ghost" :disabled="pickingDirectory" @click="pickDirectory">
            {{ pickingDirectory ? "选择中..." : "打开目录" }}
          </button>
        </div>

        <div class="chat-layout">
          <aside class="chat-sessions">
            <div class="chat-sessions-header">历史</div>
            <template v-for="group in groupedSessions" :key="group.pathKey">
              <div class="chat-session-group-header" :title="group.path ?? undefined">
                📁 {{ group.label }}
              </div>
              <div
                v-for="session in group.sessions"
                :key="session.session_id"
                class="chat-session-item"
                :class="{ active: session.session_id === activeSessionId }"
              >
                <button class="chat-session-main" @click="openSession(session.session_id)">
                  <strong>{{ session.session_id.slice(0, 8) }}</strong>
                  <span class="muted">{{ formatTime(session.updated_at) }}</span>
                </button>
                <button
                  class="chat-session-delete"
                  title="删除会话"
                  @click.stop="removeSession(session.session_id)"
                >
                  ×
                </button>
              </div>
            </template>
            <div v-if="sessions.length === 0" class="muted">暂无历史会话</div>
          </aside>

          <div class="chat-main">
            <div v-if="isWorkspaceAgent" class="workspace-path-display">
              <span class="path-label">当前工作目录：</span>
              <span class="path-value" :class="{ 'path-unset': !workspacePath }">
                {{ workspacePath || '未选择工作目录' }}
              </span>
            </div>
            <div class="chat-messages" ref="messagesContainer">
              <div v-if="messages.length === 0" class="empty-state">
                
              </div>
              <div
                v-for="message in visibleMessages"
                :key="message.id"
                class="chat-bubble-row"
                :class="message.role"
              >
                <img
                  v-if="message.role === 'assistant' && (message.agentAvatarUrl || selectedAgentAvatarUrl)"
                  class="chat-message-avatar"
                  :src="message.agentAvatarUrl || selectedAgentAvatarUrl"
                  alt="bot avatar"
                />
                <div
                  v-else-if="message.role === 'assistant'"
                  class="chat-message-avatar chat-message-avatar--fallback"
                >
                  {{ agentInitial(message.agentName || selectedAgent?.name || "Bot") }}
                </div>
                <div
                  v-if="message.role === 'assistant'"
                  class="chat-bubble-col"
                >
                  <div
                    v-if="(message.liveToolCalls && message.liveToolCalls.length > 0) || message.toolCalls.length > 0 || activeToolDetail?.messageId === message.id"
                    class="chat-tool-above-bubble"
                  >
                    <div
                      v-if="message.liveToolCalls && message.liveToolCalls.length > 0"
                      class="chat-tool-inline-list"
                    >
                      <div
                        v-for="liveCall in message.liveToolCalls"
                        :key="liveCall.call_id"
                        class="chat-live-tool-wrapper"
                      >
                        <button
                          class="chat-tool-inline"
                          :class="{ active: expandedLiveToolCalls.has(liveCall.call_id) }"
                          @click="toggleLiveToolCall(liveCall.call_id)"
                        >
                          <span v-if="!liveCall.done" class="live-tool-spinner"></span>
                          <span v-else class="live-tool-done-icon">✓</span>
                          工具调用: {{ liveCall.name }}
                        </button>
                        <div
                          v-if="expandedLiveToolCalls.has(liveCall.call_id)"
                          class="chat-tool-detail-inline"
                        >
                          <div class="chat-tool-detail-inline-block">
                            <div class="chat-tool-detail-caption">arguments</div>
                            <pre>{{ formatToolPayload(liveCall.arguments) }}</pre>
                          </div>
                          <div v-if="liveCall.done" class="chat-tool-detail-inline-block">
                            <div class="chat-tool-detail-caption">result</div>
                            <pre>{{ liveCall.result || "(空结果)" }}</pre>
                          </div>
                          <div v-else class="chat-tool-detail-inline-block">
                            <div class="chat-tool-detail-caption">result</div>
                            <div class="live-tool-pending">推理中...</div>
                          </div>
                        </div>
                      </div>
                    </div>
                    <div
                      v-if="message.toolCalls.length > 0"
                      class="chat-tool-inline-list"
                    >
                      <button
                        v-for="toolCall in message.toolCalls"
                        :key="toolCall.id"
                        class="chat-tool-inline"
                        :class="{ active: activeToolCallId === toolCall.id }"
                        @click="openToolDetail(message.id, toolCall.id)"
                      >
                        调用工具: {{ toolCall.function.name }}
                      </button>
                    </div>
                    <div
                      v-if="activeToolDetail?.messageId === message.id"
                      class="chat-tool-detail-inline"
                    >
                      <div class="chat-tool-detail-inline-header">
                        <strong>{{ activeToolDetail.toolCall.function.name }}</strong>
                        <button class="chat-tool-detail-inline-close" @click="closeToolDetail">收起</button>
                      </div>
                      <div class="chat-tool-detail-inline-block">
                        <div class="chat-tool-detail-caption">tool_call_id</div>
                        <code>{{ activeToolDetail.toolCall.id }}</code>
                      </div>
                      <div class="chat-tool-detail-inline-block">
                        <div class="chat-tool-detail-caption">arguments</div>
                        <pre>{{ formatToolPayload(activeToolDetail.toolCall.function.arguments) }}</pre>
                      </div>
                      <div class="chat-tool-detail-inline-block">
                        <div class="chat-tool-detail-caption">result</div>
                        <pre>{{ activeToolDetail.result || "(空结果)" }}</pre>
                      </div>
                    </div>
                  </div>
                  <div
                    v-if="message.thinkingContent"
                    class="chat-thinking-block"
                    :class="{ collapsed: !message.thinkingExpanded }"
                  >
                    <button
                      class="chat-thinking-toggle"
                      @click="message.thinkingExpanded = !message.thinkingExpanded"
                    >
                      <span class="chat-thinking-icon">{{ message.thinkingExpanded ? '▼' : '▶' }}</span>
                      思考过程
                      <span v-if="message.streaming && message.thinkingExpanded" class="live-tool-spinner"></span>
                    </button>
                    <div v-if="message.thinkingExpanded" class="chat-thinking-content">
                      {{ message.thinkingContent }}
                    </div>
                  </div>
                  <div
                    v-if="message.content.trim().length > 0 || message.streaming"
                    class="chat-bubble"
                    :class="message.role"
                  >
                    <div
                      class="chat-bubble-content markdown-body"
                      v-html="renderMessageContent(message.content, message.streaming)"
                    ></div>
                    <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                  </div>
                </div>
                <div v-if="message.role !== 'assistant'" class="chat-bubble" :class="message.role">
                  <div
                    class="chat-bubble-content markdown-body"
                    v-html="renderMessageContent(message.content, message.streaming)"
                  ></div>
                  <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                </div>
              </div>
            </div>

            <div class="chat-input-area">
              <div v-if="!isChatEligible" class="chat-not-supported">
                <div class="chat-not-supported-icon">🚫</div>
                <div class="chat-not-supported-title">此 Agent 不支持在 Dashboard 聊天</div>
                <div class="chat-not-supported-desc">请在 QQ 群或 HTTP Stream 端点中使用该 Agent。</div>
              </div>
              <template v-else>
                <div v-if="pendingAskUser" class="ask-user-panel">
                  <div class="ask-user-question">{{ pendingAskUser.question }}</div>
                  <div v-if="pendingAskUser.details" class="ask-user-details">
                    {{ pendingAskUser.details }}
                  </div>
                  <div class="ask-user-row">
                    <input
                      v-model="askUserAnswer"
                      type="text"
                      :placeholder="pendingAskUser.placeholder || '请输入补充信息'"
                      @input="clearChatError"
                      @keydown.enter.prevent="submitAskUserAnswer"
                    />
                    <button
                      class="btn primary"
                      :disabled="!canSubmitAskUser"
                      @click="submitAskUserAnswer"
                    >
                      提交补充信息
                    </button>
                  </div>
                </div>
                <textarea
                  v-model="draftMessage"
                  placeholder="输入消息"
                  @keydown.ctrl.enter.prevent="sendMessage"
                  @input="clearChatError"
                />
                <div class="chat-input-actions">
                  <button class="btn ghost" @click="startNewSession">新对话</button>
                  <div class="chat-input-right">
                    <div
                      v-if="isChatEligible"
                      class="chat-model-bar"
                    >
                    <div class="model-picker" :class="{ open: openPicker === 'model' }">
                      <button
                        class="model-chip"
                        @click.stop="openPicker = openPicker === 'model' ? null : 'model'"
                      >
                        {{ selectedModelLabel }}
                        <svg
                          class="chip-chevron"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2.5"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <polyline points="6 9 12 15 18 9" />
                        </svg>
                      </button>
                      <div v-if="openPicker === 'model'" class="model-picker-dropdown">
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedModelId === '' }"
                          @click.stop="selectModel('')"
                        >
                          默认模型
                        </button>
                        <button
                          v-for="model in chatModels"
                          :key="model.config_id"
                          class="model-picker-item"
                          :class="{ active: selectedModelId === model.config_id }"
                          @click.stop="selectModel(model.config_id)"
                        >
                          {{ model.name }}
                        </button>
                      </div>
                    </div>

                    <div class="model-picker" :class="{ open: openPicker === 'thinking' }">
                      <button
                        class="model-chip"
                        @click.stop="openPicker = openPicker === 'thinking' ? null : 'thinking'"
                      >
                        {{ selectedThinkingLabel }}
                        <svg
                          class="chip-chevron"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2.5"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <polyline points="6 9 12 15 18 9" />
                        </svg>
                      </button>
                      <div v-if="openPicker === 'thinking'" class="model-picker-dropdown">
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedThinkingType === '' }"
                          @click.stop="selectThinkingType('')"
                        >
                          默认{{ selectedModelLlmConfig?.thinking_type ? (selectedModelLlmConfig.thinking_type === 'enabled' ? '(启用)' : '(禁用)') : '' }}
                        </button>
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedThinkingType === 'enabled' }"
                          @click.stop="selectThinkingType('enabled')"
                        >
                          启用
                        </button>
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedThinkingType === 'disabled' }"
                          @click.stop="selectThinkingType('disabled')"
                        >
                          禁用
                        </button>
                      </div>
                    </div>

                    <div class="model-picker" :class="{ open: openPicker === 'effort' }">
                      <button
                        class="model-chip"
                        @click.stop="openPicker = openPicker === 'effort' ? null : 'effort'"
                      >
                        {{ selectedEffortLabel }}
                        <svg
                          class="chip-chevron"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2.5"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <polyline points="6 9 12 15 18 9" />
                        </svg>
                      </button>
                      <div v-if="openPicker === 'effort'" class="model-picker-dropdown">
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedReasoningEffort === '' }"
                          @click.stop="selectReasoningEffort('')"
                        >
                          默认{{ selectedModelLlmConfig?.reasoning_effort ? `(${selectedModelLlmConfig.reasoning_effort})` : '' }}
                        </button>
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedReasoningEffort === 'low' }"
                          @click.stop="selectReasoningEffort('low')"
                        >
                          low
                        </button>
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedReasoningEffort === 'medium' }"
                          @click.stop="selectReasoningEffort('medium')"
                        >
                          medium
                        </button>
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedReasoningEffort === 'high' }"
                          @click.stop="selectReasoningEffort('high')"
                        >
                          high
                        </button>
                        <button
                          class="model-picker-item"
                          :class="{ active: selectedReasoningEffort === 'max' }"
                          @click.stop="selectReasoningEffort('max')"
                        >
                          max
                        </button>
                      </div>
                    </div>

                    <div class="model-settings" :class="{ open: openPicker === 'settings' }">
                      <button
                        class="model-chip icon-only"
                        title="模型设置"
                        @click.stop="openPicker = openPicker === 'settings' ? null : 'settings'"
                      >
                        <svg
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <circle cx="12" cy="12" r="3" />
                          <path
                            d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
                          />
                        </svg>
                      </button>
                    </div>
                  </div>
                  <button
                    class="btn primary"
                    :disabled="sending || !canSend"
                    @click="sendMessage"
                  >
                    {{ sending ? "推理中..." : "发送" }}
                  </button>
                </div>
              </div>
              </template>
              <div v-if="chatErrorMessage" class="chat-error-box" role="alert">
                {{ chatErrorMessage }}
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import MarkdownIt from "markdown-it";

import {
  chat,
  system,
  type AgentWithRuntime,
  type ChatHistoryRecord,
  type ChatToolCall,
  type ChatSessionSummary,
  type ChatStreamEvent,
  type LlmConfig,
} from "../../api/client";
import { formatTime } from "../model";

type ChatRole = "user" | "assistant" | "tool";
type LiveToolCall = {
  call_id: string;
  name: string;
  arguments: unknown;
  result?: string;
  done: boolean;
};
type DashboardMessage = {
  id: string;
  role: ChatRole;
  content: string;
  thinkingContent?: string;
  thinkingExpanded?: boolean;
  streaming?: boolean;
  timestamp?: string;
  toolCalls: ChatToolCall[];
  toolCallId?: string | null;
  linkedToolCall?: ChatToolCall | null;
  agentAvatarUrl?: string;
  agentName?: string;
  liveToolCalls?: LiveToolCall[];
};
type ToolDetail = {
  messageId: string;
  toolCall: ChatToolCall;
  result: string;
};
type PendingNewConversationCommand = {
  passthroughText: string | null;
};
type PendingAskUser = {
  question: string;
  details?: string;
  placeholder?: string;
};
type StreamState = {
  assistantMessageId: string | null;
  pendingNewConversation: PendingNewConversationCommand | null;
  requestText: string;
};

const agents = ref<AgentWithRuntime[]>([]);
const agentsLoading = ref(false);
const sessions = ref<ChatSessionSummary[]>([]);
const activeSessionId = ref("");
const selectedAgentId = ref("");
const draftMessage = ref("");
const workspacePath = ref("");
const pickingDirectory = ref(false);
const sending = ref(false);
const chatErrorMessage = ref("");
const messagesContainer = ref<HTMLElement | null>(null);
const messages = ref<DashboardMessage[]>([]);
const activeToolCallId = ref("");
const expandedLiveToolCalls = ref(new Set<string>());
const llmModels = ref<LlmConfig[]>([]);
const selectedModelId = ref("");
const selectedThinkingType = ref<"" | "enabled" | "disabled">("");
const selectedReasoningEffort = ref<"" | "low" | "medium" | "high" | "max">("");
const openPicker = ref<'model' | 'thinking' | 'effort' | 'settings' | null>(null);
const stats = reactive({
  connections: 0,
  llm: 0,
  agents: 0,
});
const markdown = new MarkdownIt({
  html: false,
  breaks: true,
  linkify: true,
});

const chatEligibleAgentTypes = new Set(["http_stream", "workspace"]);
const selectedAgent = computed(() => agents.value.find((agent) => agent.config_id === selectedAgentId.value) ?? null);
const selectedAgentType = computed(() => selectedAgent.value?.agent_type?.type ?? "");
const isChatEligible = computed(() => chatEligibleAgentTypes.has(selectedAgentType.value));
const isWorkspaceAgent = computed(() => selectedAgentType.value === "workspace");
const groupedSessions = computed(() => {
  const groups = new Map<string, ChatSessionSummary[]>();
  for (const session of sessions.value) {
    const key = session.workspace_path ?? "__default__";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(session);
  }
  const result: Array<{ pathKey: string; path: string | null; label: string; sessions: ChatSessionSummary[] }> = [];
  for (const [key, items] of groups) {
    items.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
    result.push({
      pathKey: key,
      path: key === "__default__" ? null : key,
      label: key === "__default__" ? "默认路径" : key,
      sessions: items,
    });
  }
  result.sort((a, b) => b.sessions[0].updated_at.localeCompare(a.sessions[0].updated_at));
  return result;
});
const recentWorkspacePaths = computed(() => {
  const paths = new Set<string>();
  for (const session of sessions.value) {
    if (session.workspace_path) paths.add(session.workspace_path);
  }
  return Array.from(paths).slice(0, 5);
});
const chatModels = computed(() => llmModels.value.filter((item) => item.model.type === "chat_llm"));
const selectedModelLlmConfig = computed(() => {
  const modelId = selectedModelId.value || defaultAgentModelId.value;
  const model = chatModels.value.find((m) => m.config_id === modelId);
  if (model?.model.type === "chat_llm") {
    return model.model.llm;
  }
  return null;
});
const defaultAgentModelId = computed(() => {
  const agent = selectedAgent.value;
  if (!agent) {
    return "";
  }
  const agentType = agent.agent_type as Record<string, unknown>;
  return String(agentType.llm_ref_id ?? "");
});
const selectedModelLabel = computed(() => {
  if (!selectedModelId.value) {
    return "默认模型";
  }
  const model = chatModels.value.find((m) => m.config_id === selectedModelId.value);
  return model?.name ?? "默认模型";
});
const selectedThinkingLabel = computed(() => {
  const defaultType = selectedModelLlmConfig.value?.thinking_type;
  const defaultLabel = defaultType ? (defaultType === 'enabled' ? '(启用)' : '(禁用)') : '';
  if (!selectedThinkingType.value) {
    return `默认${defaultLabel}`;
  }
  return selectedThinkingType.value === 'enabled' ? '启用' : '禁用';
});
const selectedEffortLabel = computed(() => {
  const defaultEffort = selectedModelLlmConfig.value?.reasoning_effort;
  const defaultLabel = defaultEffort ? `(${defaultEffort})` : '';
  if (!selectedReasoningEffort.value) {
    return `默认${defaultLabel}`;
  }
  return selectedReasoningEffort.value;
});
const canSend = computed(() =>
  !!selectedAgent.value &&
  isChatEligible.value &&
  selectedAgent.value.runtime.status === "running" &&
  draftMessage.value.trim().length > 0,
);
const selectedAgentAvatarUrl = computed(() => agentAvatarUrl(selectedAgent.value));
const selectedAgentAvatarFallback = computed(() => {
  const name = selectedAgent.value?.name ?? "Bot";
  return agentInitial(name);
});
const pendingAskUser = ref<PendingAskUser | null>(null);
const askUserAnswer = ref("");
const canSubmitAskUser = computed(() =>
  isChatEligible.value &&
  isWorkspaceAgent.value &&
  !!pendingAskUser.value &&
  selectedAgent.value?.runtime.status === "running" &&
  askUserAnswer.value.trim().length > 0 &&
  !sending.value,
);

function parseNewConversationCommand(input: string): PendingNewConversationCommand | null {
  const match = input.trim().match(/^\/(new|clear|reset)(?:\s+([\s\S]*))?$/i);
  if (!match) {
    return null;
  }

  const passthroughText = (match[2] ?? "").trim();
  return {
    passthroughText: passthroughText.length > 0 ? passthroughText : null,
  };
}

function messageAvatarUrl(record: ChatHistoryRecord): string {
  if (record.agent_avatar_url) {
    return record.agent_avatar_url;
  }
  // Fallback: try current agent config
  const agent = agents.value.find((a) => a.config_id === record.agent_id);
  if (agent) {
    return agentAvatarUrl(agent);
  }
  return "";
}
const visibleMessages = computed(() => messages.value.filter((message) => message.role !== "tool"));
const activeToolDetail = computed<ToolDetail | null>(() => {
  if (!activeToolCallId.value) {
    return null;
  }
  for (const message of messages.value) {
    const toolCall = message.toolCalls.find((item) => item.id === activeToolCallId.value);
    if (!toolCall) {
      continue;
    }
    const resultMessage = messages.value.find(
      (item) => item.role === "tool" && item.toolCallId === toolCall.id,
    );
    return {
      messageId: message.id,
      toolCall,
      result: resultMessage?.content ?? "",
    };
  }
  return null;
});

function readableAgentType(type: string): string {
  if (type === "http_stream") {
    return "HTTP Stream Agent";
  }
  if (type === "workspace") {
    return "Workspace Agent";
  }
  return "QQ Chat Agent";
}

function agentAvatarUrl(agent: AgentWithRuntime | null | undefined): string {
  if (!agent || agent.agent_type.type !== "qq_chat") {
    return "";
  }
  const profile = agent.qq_chat_profile;
  const explicit = String(profile?.bot_avatar_url ?? "").trim();
  if (explicit) {
    return explicit;
  }
  const botUserId = String(profile?.bot_user_id ?? "").trim();
  if (!botUserId) {
    return "";
  }
  return `https://q1.qlogo.cn/g?b=qq&nk=${encodeURIComponent(botUserId)}&s=640`;
}

function agentInitial(name: string): string {
  return (name || "B").trim().slice(0, 1).toUpperCase();
}

function toApiMessages() {
  return messages.value
    .filter((item) => item.content.trim().length > 0 || item.toolCalls.length > 0 || !!item.toolCallId)
    .map((item) => ({
      role: item.role,
      content: item.content,
      tool_calls: item.toolCalls.length > 0 ? item.toolCalls : undefined,
      tool_call_id: item.toolCallId ?? undefined,
    }));
}

function applyHistory(records: ChatHistoryRecord[]) {
  const mapped: DashboardMessage[] = records
    .filter((item) => item.role === "user" || item.role === "assistant" || item.role === "tool")
    .map((item) => ({
      id: item.message_id,
      role: item.role as ChatRole,
      content: item.content,
      thinkingContent: item.reasoning_content ?? undefined,
      thinkingExpanded: !!item.reasoning_content,
      timestamp: item.timestamp,
      toolCalls: item.tool_calls ?? [],
      toolCallId: item.tool_call_id ?? null,
      linkedToolCall: null,
      agentAvatarUrl: messageAvatarUrl(item) || undefined,
      agentName: item.agent_name || undefined,
    }));
  const toolCallMap = new Map<string, ChatToolCall>();
  for (const message of mapped) {
    for (const toolCall of message.toolCalls) {
      toolCallMap.set(toolCall.id, toolCall);
    }
  }
  for (const message of mapped) {
    if (message.role === "tool" && message.toolCallId) {
      message.linkedToolCall = toolCallMap.get(message.toolCallId) ?? null;
    }
  }
  messages.value = mapped;
  if (activeToolCallId.value) {
    const stillExists = mapped.some((message) =>
      message.toolCalls.some((toolCall) => toolCall.id === activeToolCallId.value),
    );
    if (!stillExists) {
      activeToolCallId.value = "";
    }
  }
  scrollToBottom();
}

function openToolDetail(messageId: string, toolCallId: string) {
  const message = messages.value.find((item) => item.id === messageId);
  if (!message || !message.toolCalls.some((item) => item.id === toolCallId)) {
    return;
  }
  activeToolCallId.value = activeToolCallId.value === toolCallId ? "" : toolCallId;
}

function closeToolDetail() {
  activeToolCallId.value = "";
}

function toggleLiveToolCall(callId: string) {
  if (expandedLiveToolCalls.value.has(callId)) {
    expandedLiveToolCalls.value.delete(callId);
  } else {
    expandedLiveToolCalls.value.add(callId);
  }
  // Trigger Vue reactivity for Set mutation
  expandedLiveToolCalls.value = new Set(expandedLiveToolCalls.value);
}

function formatToolPayload(payload: unknown): string {
  if (payload == null) {
    return "null";
  }
  if (typeof payload === "string") {
    return payload;
  }
  try {
    return JSON.stringify(payload, null, 2);
  } catch {
    return String(payload);
  }
}

function formatChatTime(timestamp?: string): string {
  if (!timestamp) {
    return "";
  }
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return timestamp;
  }
  return date.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function renderMessageContent(content: string, streaming = false): string {
  const text = content || (streaming ? "..." : "");
  return markdown.render(text);
}

function scrollToBottom() {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight;
    }
  });
}

function clearChatError() {
  chatErrorMessage.value = "";
}

function clearPendingAskUser() {
  pendingAskUser.value = null;
  askUserAnswer.value = "";
}

function pruneFailedAssistantPlaceholder(assistantMessageId: string | null) {
  if (!assistantMessageId) {
    return;
  }
  const index = messages.value.findIndex((item) => item.id === assistantMessageId);
  if (index < 0) {
    return;
  }

  const message = messages.value[index];
  message.streaming = false;

  const hasVisibleContent = message.content.trim().length > 0 || (message.thinkingContent?.trim().length ?? 0) > 0;
  const hasToolActivity = (message.liveToolCalls?.length ?? 0) > 0 || message.toolCalls.length > 0;
  if (!hasVisibleContent && !hasToolActivity) {
    messages.value.splice(index, 1);
  }
}

function applyInferenceFailure(streamState: StreamState, errorMessage: string) {
  pruneFailedAssistantPlaceholder(streamState.assistantMessageId);
  chatErrorMessage.value = `推理失败: ${errorMessage}`;
  if (!draftMessage.value.trim()) {
    draftMessage.value = streamState.requestText;
  }
}

async function reloadSessions() {
  const result = await chat.listSessions(selectedAgentId.value || undefined);
  sessions.value = result.sessions;
}

async function openSession(sessionId: string) {
  activeSessionId.value = sessionId;
  clearChatError();
  clearPendingAskUser();
  const result = await chat.getSessionMessages(sessionId);
  // Auto-select the agent associated with this session
  const firstRecord = result.messages[0];
  if (firstRecord?.agent_id && agents.value.some((a) => a.config_id === firstRecord.agent_id)) {
    selectedAgentId.value = firstRecord.agent_id;
  }
  workspacePath.value =
    result.messages[result.messages.length - 1]?.workspace_path ??
    firstRecord?.workspace_path ??
    workspacePath.value;
  const latestRecord = result.messages[result.messages.length - 1];
  if (latestRecord?.pending_ask_user?.question) {
    pendingAskUser.value = {
      question: latestRecord.pending_ask_user.question,
      details: latestRecord.pending_ask_user.details ?? undefined,
      placeholder: latestRecord.pending_ask_user.placeholder ?? undefined,
    };
  }
  applyHistory(result.messages);
}

async function pickDirectory() {
  pickingDirectory.value = true;
  try {
    const result = await system.selectDirectory();
    if (result.path) {
      workspacePath.value = result.path;
    }
  } catch (error) {
    chatErrorMessage.value = `选择目录失败: ${(error as Error).message}`;
  } finally {
    pickingDirectory.value = false;
  }
}

function startNewSession() {
  activeSessionId.value = "";
  messages.value = [];
  activeToolCallId.value = "";
  expandedLiveToolCalls.value = new Set();
  clearChatError();
  clearPendingAskUser();
}

function selectModel(id: string) {
  selectedModelId.value = id;
  openPicker.value = null;
}

function selectThinkingType(value: "" | "enabled" | "disabled") {
  selectedThinkingType.value = value;
  openPicker.value = null;
}

function selectReasoningEffort(value: "" | "low" | "medium" | "high" | "max") {
  selectedReasoningEffort.value = value;
  openPicker.value = null;
}

function closePickersOnClickOutside(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (!target.closest(".model-picker") && !target.closest(".model-settings")) {
    openPicker.value = null;
  }
}

watch(selectedAgentId, async () => {
  await reloadSessions();
  startNewSession();
  selectedModelId.value = defaultAgentModelId.value;
  selectedThinkingType.value = "";
  selectedReasoningEffort.value = "";
  if (!isWorkspaceAgent.value) {
    workspacePath.value = "";
  }
  if (!isChatEligible.value) {
    clearPendingAskUser();
  }
});

watch(selectedModelId, () => {
  selectedThinkingType.value = "";
  selectedReasoningEffort.value = "";
});

async function removeSession(sessionId: string) {
  if (!confirm("确定要删除该会话吗？此操作不可恢复。")) {
    return;
  }
  try {
    await chat.deleteSession(sessionId);
    sessions.value = sessions.value.filter((s) => s.session_id !== sessionId);
    if (activeSessionId.value === sessionId) {
      startNewSession();
    }
  } catch (error) {
    alert(`删除失败: ${(error as Error).message}`);
  }
}

function applyStreamEvent(event: ChatStreamEvent, streamState: StreamState) {
  if (event.type === "error") {
    applyInferenceFailure(streamState, event.error ?? "未知错误");
    return;
  }

  if (event.type === "ask_user" && event.question) {
    pendingAskUser.value = {
      question: event.question,
      details: event.details ?? undefined,
      placeholder: event.placeholder ?? undefined,
    };
    askUserAnswer.value = "";
    return;
  }

  if (event.type === "start") {
    if (streamState.pendingNewConversation) {
      startNewSession();
      if (event.session_id) {
        activeSessionId.value = event.session_id;
      }

      const passthroughText = streamState.pendingNewConversation.passthroughText;
      if (passthroughText) {
        messages.value.push({
          id: `local-user-${crypto.randomUUID()}`,
          role: "user",
          content: passthroughText,
          timestamp: new Date().toISOString(),
          toolCalls: [],
          toolCallId: null,
          linkedToolCall: null,
        });

        const assistantMessageId = event.message_id ?? `local-assistant-${crypto.randomUUID()}`;
        messages.value.push({
          id: assistantMessageId,
          role: "assistant",
          content: "",
          streaming: true,
          timestamp: new Date().toISOString(),
          toolCalls: [],
          toolCallId: null,
          linkedToolCall: null,
        });
        streamState.assistantMessageId = assistantMessageId;
      } else {
        streamState.assistantMessageId = event.message_id ?? null;
      }

      streamState.pendingNewConversation = null;
      scrollToBottom();
      return;
    }

    if (event.session_id) {
      activeSessionId.value = event.session_id;
    }
    if (event.message_id) {
      const currentAssistantId = streamState.assistantMessageId;
      const message = currentAssistantId
        ? messages.value.find((item) => item.id === currentAssistantId || item.id === event.message_id)
        : undefined;
      if (message) {
        message.id = event.message_id;
      }
      streamState.assistantMessageId = event.message_id;
    }
  }

  if (event.type === "delta") {
    const targetId = event.message_id || streamState.assistantMessageId;
    if (!targetId) {
      return;
    }
    const message = messages.value.find((item) => item.id === targetId);
    if (message) {
      message.content += event.token ?? "";
      message.streaming = true;
      scrollToBottom();
    }
  }

  if (event.type === "thinking_delta") {
    const targetId = event.message_id || streamState.assistantMessageId;
    if (!targetId) {
      return;
    }
    const message = messages.value.find((item) => item.id === targetId);
    if (message) {
      if (!message.thinkingContent) {
        message.thinkingContent = "";
        message.thinkingExpanded = true;
      }
      message.thinkingContent += event.token ?? "";
      message.streaming = true;
      scrollToBottom();
    }
  }

  if (event.type === "done") {
    const targetId = event.message_id || streamState.assistantMessageId;
    if (!targetId) {
      return;
    }
    const message = messages.value.find((item) => item.id === targetId);
    if (message) {
      message.streaming = false;
    }
  }

  if (event.type === "tool_call_start" && event.call_id && event.name) {
    const targetId = event.message_id || streamState.assistantMessageId;
    if (!targetId) {
      return;
    }
    const message = messages.value.find((item) => item.id === targetId);
    if (message) {
      if (!message.liveToolCalls) {
        message.liveToolCalls = [];
      }
      message.liveToolCalls.push({
        call_id: event.call_id,
        name: event.name,
        arguments: event.arguments,
        done: false,
      });
      scrollToBottom();
    }
  }

  if (event.type === "tool_call_result" && event.call_id) {
    const targetId = event.message_id || streamState.assistantMessageId;
    if (!targetId) {
      return;
    }
    const message = messages.value.find((item) => item.id === targetId);
    if (message?.liveToolCalls) {
      const liveCall = message.liveToolCalls.find((item) => item.call_id === event.call_id);
      if (liveCall) {
        liveCall.result = event.result ?? "";
        liveCall.done = true;
        scrollToBottom();
      }
    }
  }
}

async function sendMessage() {
  await sendMessageWithText(draftMessage.value, false);
}

async function submitAskUserAnswer() {
  await sendMessageWithText(askUserAnswer.value, true);
}

async function sendMessageWithText(rawInput: string, fromAskUser: boolean) {
  if (sending.value) {
    return;
  }

  const userText = rawInput.trim();
  if (!userText) {
    return;
  }
  if (!selectedAgent.value || selectedAgent.value.runtime.status !== "running") {
    return;
  }
  if (!fromAskUser && !canSend.value) {
    return;
  }
  if (fromAskUser && !canSubmitAskUser.value) {
    return;
  }
  clearChatError();
  const pendingNewConversation = parseNewConversationCommand(userText);
  const requestMessages = [
    ...toApiMessages(),
    {
      role: "user",
      content: userText,
    },
  ];

  if (fromAskUser) {
    askUserAnswer.value = "";
  } else {
    draftMessage.value = "";
  }
  sending.value = true;

  const streamState: StreamState = {
    assistantMessageId: null,
    pendingNewConversation,
    requestText: userText,
  };

  if (!pendingNewConversation) {
    const userMessage = {
      id: `local-user-${crypto.randomUUID()}`,
      role: "user" as const,
      content: userText,
      timestamp: new Date().toISOString(),
      toolCalls: [],
      toolCallId: null,
      linkedToolCall: null,
    };
    messages.value.push(userMessage);

    const assistantTempId = `local-assistant-${crypto.randomUUID()}`;
    messages.value.push({
      id: assistantTempId,
      role: "assistant",
      content: "",
      streaming: true,
      timestamp: new Date().toISOString(),
      toolCalls: [],
      toolCallId: null,
      linkedToolCall: null,
    });
    streamState.assistantMessageId = assistantTempId;
    scrollToBottom();
  }

  try {
    await chat.stream(
      {
        agent_id: selectedAgentId.value,
        session_id: activeSessionId.value || null,
        stream: true,
        model_config_id: selectedModelId.value || null,
        thinking_type: selectedThinkingType.value || null,
        reasoning_effort: selectedReasoningEffort.value || null,
        workspace_path: isWorkspaceAgent.value ? workspacePath.value.trim() || null : null,
        messages: requestMessages,
      },
      (event) => applyStreamEvent(event, streamState),
    );
    await reloadSessions();
    if (activeSessionId.value) {
      await openSession(activeSessionId.value);
    }
  } catch (error) {
    applyInferenceFailure(streamState, (error as Error).message);
  } finally {
    sending.value = false;
  }
}

async function load() {
  agentsLoading.value = true;
  try {
    const [connections, llm, loadedAgents] = await Promise.all([
      system.connections.list(),
      system.llm.list(),
      system.agents.list(),
    ]);
    stats.connections = connections.length;
    stats.llm = llm.length;
    stats.agents = loadedAgents.length;
    agents.value = loadedAgents;
    llmModels.value = llm;

    if (!selectedAgentId.value || !loadedAgents.some((agent) => agent.config_id === selectedAgentId.value)) {
      const firstEligible = loadedAgents.find((agent) => chatEligibleAgentTypes.has(agent.agent_type.type));
      selectedAgentId.value = firstEligible?.config_id ?? loadedAgents[0]?.config_id ?? "";
    }
    selectedModelId.value = defaultAgentModelId.value;
  } finally {
    agentsLoading.value = false;
  }

  await reloadSessions();
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`仪表盘加载失败: ${(error as Error).message}`);
  });
  document.addEventListener("click", closePickersOnClickOutside);
});

onUnmounted(() => {
  document.removeEventListener("click", closePickersOnClickOutside);
});
</script>

<style scoped lang="scss">
.dashboard-page {
  gap: 16px;
  display: flex;
  flex-direction: column;
  height: calc(100vh - 40px); /* 占满窗口 */
}

.dashboard-stats {
  display: flex;
  align-items: center;
  gap: 24px;
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
}

.stat-item .muted {
  font-size: 13px;
  margin-bottom: 4px;
}

.stat-item strong {
  font-size: 24px;
  line-height: 1;
  color: var(--admin-accent);
}

.stat-divider {
  width: 1px;
  height: 36px;
  background: var(--admin-border);
}

.dashboard-panels {
  gap: 16px;
  align-items: start;
  grid-template-columns: 1fr;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.chat-panel {
  flex: 1;
  min-height: 0;
  width: 100%;
  display: flex;
  flex-direction: column;
  padding: 24px;
}

.chat-toolbar {
  display: flex;
  gap: 16px;
  justify-content: space-between;
  align-items: flex-start;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--admin-border);
}

.chat-agent-picker {
  display: grid;
  gap: 10px;
  min-width: 420px;
  flex: 1;
}

.chat-agent-picker-title {
  margin-bottom: 0;
  font-weight: 500;
  color: var(--admin-muted);
}

.chat-agent-cards {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.chat-agent-loading {
  min-height: 54px;
  display: flex;
  align-items: center;
  gap: 9px;
  color: var(--admin-subtle);
}

.chat-agent-loading-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid color-mix(in srgb, var(--admin-accent) 28%, transparent);
  border-top-color: var(--admin-accent);
  border-radius: 50%;
  animation: spin 0.75s linear infinite;
  flex-shrink: 0;
}

.chat-agent-card {
  border: 1px solid var(--admin-border);
  background: var(--admin-bg-soft);
  border-radius: 12px;
  padding: 8px 10px;
  min-width: 220px;
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.chat-agent-card:hover {
  border-color: color-mix(in srgb, var(--admin-accent) 32%, var(--admin-border) 68%);
  transform: translateY(-1px);
}

.chat-agent-card.active {
  border-color: color-mix(in srgb, var(--admin-accent) 52%, var(--admin-border) 48%);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--admin-accent-soft) 70%, transparent 30%);
  background: color-mix(in srgb, var(--admin-bg-elevated) 82%, var(--admin-accent-soft) 18%);
}

.chat-agent-card-avatar {
  width: 40px;
  height: 40px;
  border-radius: 999px;
  object-fit: cover;
  border: 1px solid var(--admin-border);
  flex-shrink: 0;
}

.chat-agent-card-avatar--fallback {
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--admin-accent) 18%, var(--admin-bg-panel) 82%);
  color: var(--admin-ink);
  font-weight: 700;
}

.chat-agent-card-meta {
  display: grid;
  text-align: left;
  min-width: 0;
}

.chat-agent-card-meta strong {
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chat-agent-card-meta span {
  font-size: 12px;
  color: var(--admin-subtle);
}

.chat-agent-card.inactive {
  opacity: 0.55;
  border-color: var(--admin-border);
}

.chat-agent-card.inactive:hover {
  opacity: 0.8;
}

.chat-agent-card.unsupported {
  cursor: not-allowed;
  opacity: 0.45;
}

.chat-agent-card.unsupported:hover {
  transform: none;
  border-color: var(--admin-border);
}

.agent-status-badge {
  background: var(--admin-danger, #ef4444);
  color: white;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 7px;
  border-radius: 5px;
  margin-left: auto;
  flex-shrink: 0;
}

.agent-status-badge.unsupported-badge {
  background: var(--admin-muted, #6b7280);
}

.chat-layout {
  margin-top: 16px;
  display: grid;
  grid-template-columns: 240px 1fr;
  gap: 20px;
  flex: 1;
  min-height: 0;
}

.chat-sessions {
  background: var(--admin-bg-panel);
  border: 1px solid var(--admin-border);
  border-radius: 16px;
  padding: 14px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow-y: auto;
}

.chat-sessions-header {
  font-size: 13px;
  font-weight: 600;
  color: var(--admin-muted);
  padding: 4px 8px;
  margin-bottom: 4px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.chat-session-item {
  border: 1px solid transparent;
  background: transparent;
  color: var(--admin-ink);
  border-radius: 10px;
  padding: 6px 8px 6px 14px;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.chat-session-item:hover {
  background: var(--admin-bg-soft);
}

.chat-session-item.active {
  background: var(--admin-bg-elevated);
  border-color: var(--admin-border);
  box-shadow: 0 4px 12px color-mix(in srgb, var(--admin-bg) 60%, transparent);
}

.chat-session-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: transparent;
  border: none;
  padding: 0;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.chat-session-main strong {
  font-size: 14px;
  font-family: monospace;
}

.chat-session-main .muted {
  font-size: 12px;
  color: var(--admin-subtle);
}

.chat-session-delete {
  width: 24px;
  height: 24px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--admin-subtle);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  display: grid;
  place-items: center;
  flex-shrink: 0;
  opacity: 0;
  transition: all 0.2s ease;
}

.chat-session-item:hover .chat-session-delete {
  opacity: 1;
}

.chat-session-delete:hover {
  background: color-mix(in srgb, var(--admin-danger, #ef4444) 12%, transparent);
  color: var(--admin-danger, #ef4444);
}

.chat-main {
  background: var(--admin-bg-panel);
  border: 1px solid var(--admin-border);
  border-radius: 16px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 4px 24px color-mix(in srgb, var(--admin-bg) 60%, transparent);
}

.chat-messages {
  flex: 1;
  min-height: 0;
  padding: 24px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.chat-bubble-row {
  display: flex;
  align-items: flex-end;
  gap: 10px;
}

.chat-bubble-row.user {
  justify-content: flex-end;
}

.chat-bubble-row.assistant {
  justify-content: flex-start;
}

.chat-bubble-col {
  display: flex;
  flex-direction: column;
  max-width: calc(85% - 54px);
}

.chat-tool-above-bubble {
  margin-bottom: 6px;
}

.chat-message-avatar {
  width: 40px;
  height: 40px;
  border-radius: 999px;
  object-fit: cover;
  border: 1px solid var(--admin-border);
  flex-shrink: 0;
  margin-bottom: 2px;
}

.chat-message-avatar--fallback {
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--admin-accent) 16%, var(--admin-bg-panel) 84%);
  color: var(--admin-ink);
  font-size: 13px;
  font-weight: 700;
}

.chat-bubble {
  max-width: 85%;
  border-radius: 18px;
  padding: 12px 18px;
  line-height: 1.6;
  font-size: 15px;
  box-shadow: 0 2px 8px color-mix(in srgb, var(--admin-bg) 40%, transparent);
  overflow-wrap: anywhere;
}

.chat-bubble-col .chat-bubble {
  max-width: 100%;
}

.chat-thinking-block {
  margin-bottom: 10px;
  border: 1px solid var(--admin-border);
  border-radius: 10px;
  overflow: hidden;
  background: color-mix(in srgb, var(--admin-accent) 6%, var(--admin-bg-panel) 94%);
}

.chat-thinking-block.collapsed {
  background: transparent;
  border-color: transparent;
}

.chat-thinking-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  background: none;
  border: none;
  padding: 6px 10px;
  font-size: 13px;
  color: var(--admin-subtle);
  cursor: pointer;
  text-align: left;
  line-height: 1.4;
}

.chat-thinking-toggle:hover {
  color: var(--admin-accent);
}

.chat-thinking-icon {
  font-size: 10px;
  flex-shrink: 0;
}

.chat-thinking-content {
  padding: 8px 12px 10px;
  font-size: 13px;
  color: var(--admin-subtle);
  line-height: 1.5;
  white-space: pre-wrap;
  border-top: 1px solid var(--admin-border);
}

.chat-bubble-time {
  margin-top: 6px;
  font-size: 12px;
  color: var(--admin-subtle);
  opacity: 0.85;
  text-align: right;
}

.chat-bubble.assistant .chat-bubble-time {
  text-align: left;
}

.chat-bubble.user {
  background: var(--admin-accent);
  color: #fff;
  border-bottom-right-radius: 4px;
}

.chat-bubble.assistant {
  background: var(--admin-bg-elevated);
  border: 1px solid var(--admin-border);
  color: var(--admin-ink);
  border-bottom-left-radius: 4px;
}

.chat-tool-inline-list {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
}

.chat-tool-inline {
  border: none;
  background: transparent;
  padding: 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--admin-subtle);
  cursor: pointer;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 5px;
}

.chat-tool-inline:hover,
.chat-tool-inline.active {
  color: var(--admin-accent);
  text-decoration: underline;
}

.chat-live-tool-wrapper {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  width: 100%;
}

.live-tool-spinner {
  display: inline-block;
  width: 10px;
  height: 10px;
  border: 2px solid color-mix(in srgb, var(--admin-accent) 40%, transparent);
  border-top-color: var(--admin-accent);
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
  flex-shrink: 0;
}

.live-tool-done-icon {
  color: var(--admin-accent);
  font-size: 11px;
  font-weight: 700;
  flex-shrink: 0;
}

.live-tool-pending {
  font-size: 12px;
  color: var(--admin-subtle);
  font-style: italic;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.chat-tool-detail-inline {
  margin-top: 10px;
  padding: 12px 14px;
  background: color-mix(in srgb, var(--admin-bg-panel) 74%, var(--admin-accent-soft) 26%);
  border: 1px dashed color-mix(in srgb, var(--admin-accent) 42%, var(--admin-border) 58%);
  border-radius: 12px;
  display: grid;
  gap: 10px;
}

.chat-tool-detail-inline-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.chat-tool-detail-inline-close {
  border: none;
  background: transparent;
  padding: 0;
  font-size: 12px;
  color: var(--admin-subtle);
  cursor: pointer;
}

.chat-tool-detail-inline-close:hover {
  color: var(--admin-accent);
}

.chat-tool-detail-caption {
  font-size: 12px;
  color: var(--admin-subtle);
  margin-bottom: 4px;
}

.chat-tool-detail-inline-block {
  min-width: 0;
}

.chat-tool-detail-inline pre {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  background: color-mix(in srgb, var(--admin-bg) 82%, black 18%);
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  padding: 12px 14px;
  max-height: 180px;
  overflow: auto;
}

.chat-input-area {
  border-top: 1px solid var(--admin-border);
  padding: 16px 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: var(--admin-bg-elevated);
}

.workspace-path-bar,
.ask-user-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  background: color-mix(in srgb, var(--admin-bg-panel) 88%, var(--admin-bg) 12%);
}

.workspace-path-bar label,
.ask-user-question {
  font-size: 13px;
  font-weight: 700;
  color: var(--admin-ink);
}

.workspace-path-bar input,
.ask-user-row input {
  background: var(--admin-bg);
  color: var(--admin-ink);
  border: 1px solid var(--admin-border);
  border-radius: 10px;
  padding: 10px 12px;
}

.workspace-path-row {
  display: flex;
  gap: 8px;
}

.workspace-path-row input {
  flex: 1;
}

.workspace-browse-btn {
  flex-shrink: 0;
  white-space: nowrap;
}

.workspace-path-display {
  padding: 10px 16px;
  background: var(--admin-bg-elevated);
  border-bottom: 1px solid var(--admin-border);
  display: flex;
  align-items: center;
  gap: 8px;
}

.workspace-path-display .path-label {
  font-size: 13px;
  color: var(--admin-subtle);
  font-weight: 500;
  white-space: nowrap;
}

.workspace-path-display .path-value {
  font-size: 13px;
  color: var(--admin-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.workspace-path-display .path-value.path-unset {
  color: var(--admin-muted);
  font-style: italic;
}

.workspace-recent-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.workspace-recent-label {
  font-size: 12px;
  color: var(--admin-muted);
  margin-right: 2px;
}

.workspace-recent-chip {
  font-size: 12px;
  padding: 3px 10px;
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  background: var(--admin-bg-soft);
  color: var(--admin-muted);
  cursor: pointer;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: all 0.15s;
}

.workspace-recent-chip:hover {
  border-color: var(--admin-accent);
  color: var(--admin-ink);
}

.workspace-recent-chip.active {
  border-color: var(--admin-accent);
  color: var(--admin-accent);
  background: color-mix(in srgb, var(--admin-accent) 12%, transparent 88%);
}

.chat-session-group-header {
  font-size: 12px;
  color: var(--admin-muted);
  padding: 6px 10px 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-bottom: 1px solid var(--admin-border);
  margin-top: 4px;
}

.chat-session-group-header:first-child {
  margin-top: 0;
}

.ask-user-details {
  color: var(--admin-muted);
  font-size: 13px;
  line-height: 1.5;
  white-space: pre-wrap;
}

.ask-user-row {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.ask-user-row input {
  flex: 1 1 320px;
}

.chat-input-area textarea {
  background: var(--admin-bg);
  color: var(--admin-ink);
  border: 1px solid var(--admin-border);
  resize: none;
  height: 80px;
  border-radius: 12px;
  padding: 12px 16px;
  font-size: 15px;
  line-height: 1.5;
  transition: all 0.2s;
}

.chat-input-area textarea:focus {
  border-color: var(--admin-accent);
  outline: none;
  box-shadow: 0 0 0 3px var(--admin-accent-soft);
}

.chat-not-supported {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 32px 20px;
  text-align: center;
  color: var(--admin-muted);
}

.chat-not-supported-icon {
  font-size: 32px;
  line-height: 1;
  opacity: 0.7;
}

.chat-not-supported-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--admin-ink);
}

.chat-not-supported-desc {
  font-size: 13px;
  color: var(--admin-subtle);
}

.chat-input-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
}

.chat-error-box {
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid color-mix(in srgb, var(--admin-danger, #ef4444) 40%, var(--admin-border) 60%);
  background: color-mix(in srgb, var(--admin-danger, #ef4444) 12%, var(--admin-bg-panel) 88%);
  color: color-mix(in srgb, var(--admin-danger, #ef4444) 82%, var(--admin-ink) 18%);
  font-size: 13px;
  line-height: 1.5;
  white-space: pre-wrap;
}

.chat-input-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.chat-model-bar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.model-picker {
  position: relative;
}

.model-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: var(--admin-bg);
  border: 1px solid var(--admin-border);
  border-radius: 10px;
  padding: 6px 12px;
  font-size: 13px;
  font-weight: 500;
  color: var(--admin-ink);
  cursor: pointer;
  transition: all 0.2s ease;
  white-space: nowrap;
  line-height: 1;
}

.model-chip:hover {
  border-color: color-mix(in srgb, var(--admin-accent) 40%, var(--admin-border) 60%);
  background: var(--admin-bg-elevated);
}

.model-chip.icon-only {
  padding: 6px;
  color: var(--admin-muted);
}

.model-chip.icon-only:hover {
  color: var(--admin-accent);
}

.model-chip svg {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.chip-chevron {
  transition: transform 0.2s ease;
}

.model-picker.open .chip-chevron {
  transform: rotate(180deg);
}

.model-picker-dropdown {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 200px;
  max-height: 280px;
  overflow-y: auto;
  background: var(--admin-bg-panel);
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  box-shadow: 0 8px 24px color-mix(in srgb, var(--admin-bg) 60%, transparent);
  z-index: 10;
}

.model-picker-item {
  background: transparent;
  border: none;
  border-radius: 8px;
  padding: 8px 10px;
  font-size: 13px;
  color: var(--admin-ink);
  text-align: left;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}

.model-picker-item:hover {
  background: var(--admin-bg-soft);
}

.model-picker-item.active {
  background: color-mix(in srgb, var(--admin-accent) 12%, var(--admin-bg-soft) 88%);
  color: var(--admin-accent);
  font-weight: 600;
}

.model-settings {
  position: relative;
}

.model-settings-popover {
  position: absolute;
  bottom: calc(100% + 6px);
  right: 0;
  min-width: 220px;
  background: var(--admin-bg-panel);
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  box-shadow: 0 8px 24px color-mix(in srgb, var(--admin-bg) 60%, transparent);
  z-index: 10;
}

.model-settings-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.model-settings-row label {
  font-size: 12px;
  font-weight: 600;
  color: var(--admin-muted);
}

.model-settings-row select {
  background: var(--admin-bg);
  color: var(--admin-ink);
  border: 1px solid var(--admin-border);
  border-radius: 8px;
  padding: 6px 8px;
  font-size: 13px;
  cursor: pointer;
}

.model-settings-row select:focus {
  border-color: var(--admin-accent);
  outline: none;
}

.chat-bubble-content :deep(*) {
  margin: 0;
}

.chat-bubble-content :deep(* + *) {
  margin-top: 0.75em;
}

.chat-bubble-content :deep(p),
.chat-bubble-content :deep(li) {
  white-space: pre-wrap;
}

.chat-bubble-content :deep(ul),
.chat-bubble-content :deep(ol) {
  padding-left: 1.4em;
}

.chat-bubble-content :deep(blockquote) {
  padding-left: 12px;
  border-left: 3px solid color-mix(in srgb, var(--admin-accent) 45%, transparent);
  color: var(--admin-subtle);
}

.chat-bubble-content :deep(code) {
  font-family: "Cascadia Code", "JetBrains Mono", Consolas, monospace;
  font-size: 0.92em;
  padding: 0.15em 0.4em;
  border-radius: 6px;
  background: color-mix(in srgb, var(--admin-bg) 82%, black 18%);
}

.chat-bubble-content :deep(pre) {
  overflow-x: auto;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--admin-border);
  background: color-mix(in srgb, var(--admin-bg) 82%, black 18%);
}

.chat-bubble-content :deep(pre code) {
  display: block;
  padding: 0;
  background: transparent;
  white-space: pre;
}

.chat-bubble-content :deep(a) {
  color: inherit;
  text-decoration: underline;
}

.chat-bubble-content :deep(hr) {
  border: 0;
  border-top: 1px solid var(--admin-border);
}

@media (max-width: 1180px) {
  .chat-toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .chat-agent-picker {
    min-width: 0;
  }

  .chat-layout {
    grid-template-columns: 1fr;
  }

  .chat-sessions {
    max-height: 240px;
  }
}

@media (max-width: 900px) {
  .dashboard-page {
    height: auto;
    min-height: calc(100vh - 32px);
  }

  .dashboard-stats {
    flex-wrap: wrap;
    gap: 14px;
  }

  .stat-divider {
    display: none;
  }

  .chat-panel {
    padding: 18px;
  }

  .chat-agent-card {
    min-width: min(220px, 100%);
    flex: 1 1 220px;
  }

  .chat-messages {
    padding: 18px;
  }

  .chat-input-area {
    padding: 14px 16px;
  }

  .chat-input-actions {
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .chat-bubble,
  .chat-bubble-col .chat-bubble {
    max-width: 100%;
  }
}
</style>
