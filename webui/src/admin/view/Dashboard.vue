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
              <button
                v-for="agent in agents"
                :key="agent.id"
                class="chat-agent-card"
                :class="{ active: selectedAgentId === agent.id, inactive: agent.runtime.status !== 'running' }"
                @click="selectedAgentId = agent.id"
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
                <span v-if="agent.runtime.status !== 'running'" class="agent-status-badge">未运行</span>
              </button>
            </div>
          </div>
          <button class="btn ghost" @click="reloadSessions">刷新历史</button>
        </div>

        <div class="chat-layout">
          <aside class="chat-sessions">
            <div class="chat-sessions-header">历史</div>
            <div
              v-for="session in sessions"
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
            <div v-if="sessions.length === 0" class="muted">暂无历史会话</div>
          </aside>

          <div class="chat-main">
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
                <div class="chat-bubble" :class="message.role">
                  <div
                    class="chat-bubble-content markdown-body"
                    v-html="renderMessageContent(message.content, message.streaming)"
                  ></div>
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
                  <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                </div>
              </div>
            </div>

            <div class="chat-input-area">
              <textarea
                v-model="draftMessage"
                placeholder="输入消息"
                @keydown.ctrl.enter.prevent="sendMessage"
              />
              <div class="chat-input-actions">
                <button class="btn ghost" @click="startNewSession">新对话</button>
                <button class="btn primary" :disabled="sending || !canSend" @click="sendMessage">
                  {{ sending ? "推理中..." : "发送" }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref, watch } from "vue";
import MarkdownIt from "markdown-it";

import {
  chat,
  system,
  type AgentWithRuntime,
  type ChatHistoryRecord,
  type ChatToolCall,
  type ChatSessionSummary,
  type ChatStreamEvent,
} from "../../api/client";
import { formatTime } from "../model";

type ChatRole = "user" | "assistant" | "tool";
type DashboardMessage = {
  id: string;
  role: ChatRole;
  content: string;
  streaming?: boolean;
  timestamp?: string;
  toolCalls: ChatToolCall[];
  toolCallId?: string | null;
  linkedToolCall?: ChatToolCall | null;
  agentAvatarUrl?: string;
  agentName?: string;
};
type ToolDetail = {
  messageId: string;
  toolCall: ChatToolCall;
  result: string;
};

const agents = ref<AgentWithRuntime[]>([]);
const sessions = ref<ChatSessionSummary[]>([]);
const activeSessionId = ref("");
const selectedAgentId = ref("");
const draftMessage = ref("");
const sending = ref(false);
const messagesContainer = ref<HTMLElement | null>(null);
const messages = ref<DashboardMessage[]>([]);
const activeToolCallId = ref("");
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

const selectedAgent = computed(() => agents.value.find((agent) => agent.id === selectedAgentId.value) ?? null);
const canSend = computed(() =>
  !!selectedAgent.value &&
  selectedAgent.value.runtime.status === "running" &&
  draftMessage.value.trim().length > 0,
);
const selectedAgentAvatarUrl = computed(() => agentAvatarUrl(selectedAgent.value));
const selectedAgentAvatarFallback = computed(() => {
  const name = selectedAgent.value?.name ?? "Bot";
  return agentInitial(name);
});

function messageAvatarUrl(record: ChatHistoryRecord): string {
  if (record.agent_avatar_url) {
    return record.agent_avatar_url;
  }
  // Fallback: try current agent config
  const agent = agents.value.find((a) => a.id === record.agent_id);
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
  return type === "http_stream" ? "HTTP Stream Agent" : "QQ Chat Agent";
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
  const mapped = records
    .filter((item) => item.role === "user" || item.role === "assistant" || item.role === "tool")
    .map((item) => ({
      id: item.message_id,
      role: item.role as ChatRole,
      content: item.content,
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

async function reloadSessions() {
  const result = await chat.listSessions(selectedAgentId.value || undefined);
  sessions.value = result.sessions;
}

async function openSession(sessionId: string) {
  activeSessionId.value = sessionId;
  const result = await chat.getSessionMessages(sessionId);
  // Auto-select the agent associated with this session
  const firstRecord = result.messages[0];
  if (firstRecord?.agent_id && agents.value.some((a) => a.id === firstRecord.agent_id)) {
    selectedAgentId.value = firstRecord.agent_id;
  }
  applyHistory(result.messages);
}

function startNewSession() {
  activeSessionId.value = "";
  messages.value = [];
  activeToolCallId.value = "";
}

watch(selectedAgentId, async () => {
  await reloadSessions();
  startNewSession();
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

function applyStreamEvent(event: ChatStreamEvent, assistantTempId: string) {
  if (event.type === "start") {
    if (event.session_id) {
      activeSessionId.value = event.session_id;
    }
    if (event.message_id) {
      const message = messages.value.find((item) => item.id === assistantTempId);
      if (message) {
        message.id = event.message_id;
      }
    }
  }

  if (event.type === "delta") {
    const targetId = event.message_id || assistantTempId;
    const message = messages.value.find((item) => item.id === targetId || item.id === assistantTempId);
    if (message) {
      message.content += event.token ?? "";
      message.streaming = true;
      scrollToBottom();
    }
  }

  if (event.type === "done") {
    const message = messages.value.find((item) => item.id === (event.message_id || assistantTempId) || item.id === assistantTempId);
    if (message) {
      message.streaming = false;
    }
  }
}

async function sendMessage() {
  if (!canSend.value || sending.value) {
    return;
  }

  const userText = draftMessage.value.trim();
  draftMessage.value = "";
  sending.value = true;

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
  const requestMessages = toApiMessages();

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
  scrollToBottom();

  try {
    await chat.stream(
      {
        agent_id: selectedAgentId.value,
        session_id: activeSessionId.value || null,
        stream: true,
        messages: requestMessages,
      },
      (event) => applyStreamEvent(event, assistantTempId),
    );
    await reloadSessions();
    if (activeSessionId.value) {
      await openSession(activeSessionId.value);
    }
  } catch (error) {
    const message = messages.value.find((item) => item.id === assistantTempId);
    if (message) {
      message.streaming = false;
      if (!message.content) {
        message.content = `推理失败: ${(error as Error).message}`;
      }
    }
  } finally {
    sending.value = false;
  }
}

async function load() {
  const [connections, llm, loadedAgents] = await Promise.all([
    system.connections.list(),
    system.llm.list(),
    system.agents.list(),
  ]);
  stats.connections = connections.length;
  stats.llm = llm.length;
  stats.agents = loadedAgents.length;
  agents.value = loadedAgents;

  if (!selectedAgentId.value || !loadedAgents.some((agent) => agent.id === selectedAgentId.value)) {
    selectedAgentId.value = loadedAgents[0]?.id ?? "";
  }

  await reloadSessions();
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`仪表盘加载失败: ${(error as Error).message}`);
  });
});
</script>

<style scoped>
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

.chat-bubble-row.assistant .chat-bubble {
  max-width: calc(85% - 54px);
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
}

.chat-tool-inline:hover,
.chat-tool-inline.active {
  color: var(--admin-accent);
  text-decoration: underline;
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

.chat-input-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
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
</style>
