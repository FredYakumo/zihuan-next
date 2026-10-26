import { computed, onUnmounted, ref } from "vue";

import {
  explorer,
  type ServiceChatContextMedia,
  type ServiceChatContextQuoted,
  type ServiceChatContextResponse,
  type ServiceChatContextTurn,
  type ServiceChatUserItem,
} from "../../api/client";
import type {
  QqChatContextEvent,
  QqContextQuoted,
  QqSubAgentEntry,
  TokenUsage,
} from "../../api/types";
import { ws } from "../../api/ws";

export type ContextBackend = "memory" | "redis";

export interface ContextStageItem {
  kind: "stage";
  id: string;
  at: string;
  stage: string;
  detail: unknown;
}

export interface ContextToolItem {
  kind: "tool";
  id: string;
  callId: string;
  name: string;
  arguments: string;
  result?: string;
  durationMs?: number;
  running: boolean;
}

/**
 * One step of a sub-agent's own tool-calling loop. `path` names the enclosing tool calls,
 * outermost first, so the box it renders in can be nested and indented.
 */
export interface ContextSubAgentItem {
  kind: "sub-agent";
  id: string;
  at: string;
  path: string[];
  event: QqSubAgentEntry;
}

/** A stage or tool call, kept in the order it happened so the flow reads chronologically. */
export type ContextEntryItem = ContextStageItem | ContextToolItem | ContextSubAgentItem;

/**
 * Mirrors the backend tool failure envelopes: `{"ok":false,...}` or `{"error":...}`.
 * Plain-text or other JSON results count as successes.
 */
export function toolResultFailed(result?: string): boolean {
  if (!result) return false;
  try {
    const data: unknown = JSON.parse(result.trim());
    if (data && typeof data === "object" && !Array.isArray(data)) {
      const record = data as Record<string, unknown>;
      return record.ok === false || typeof record.error === "string";
    }
  } catch {
    // Not JSON — a plain-text result is a success.
  }
  return false;
}

/** One turn of the conversation: the user message plus everything that followed it. */
export interface ContextQuotedMessage {
  messageId: string;
  senderName?: string;
  senderId?: string;
  text: string;
}

export interface ContextTurnView {
  turnId: string;
  createdAt: string;
  userText: string;
  /** Group the user message was sent in; only present for group chats. */
  groupName?: string;
  /** Quoted message shown as a collapsible block inside the user bubble. */
  quoted?: ContextQuotedMessage;
  media: ServiceChatContextMedia[];
  entries: ContextEntryItem[];
  assistantText?: string;
  /** Model thinking for the turn, rendered as a collapsed block like the Chat page. */
  reasoningText?: string;
  usage?: TokenUsage;
  generationMs?: number;
  tokensPerSecond?: number;
  /** Steer follow-up of the previous turn: shown in the same block, without a divider. */
  continuesPrevious: boolean;
  finished: boolean;
}

export function useChatContext(serviceId: string) {
  const backend = ref<ContextBackend>("memory");
  const loadingUsers = ref(false);
  const users = ref<ServiceChatUserItem[]>([]);
  const selectedSenderId = ref("");
  const selectedSenderName = ref("");

  const loadingContext = ref(false);
  const clearingContext = ref(false);
  const turns = ref<ContextTurnView[]>([]);
  /**
   * Partly written sub-agent messages, keyed by `turn id + call path`. A live stream keeps
   * appending to the same entry; the settled message removes it.
   */
  const liveChunkEntries = new Map<string, ContextSubAgentItem>();
  /** Turns restored from the relational archive; shown above a divider, before the live window. */
  const archivedTurns = ref<ContextTurnView[]>([]);
  const inputTokens = ref(0);
  const outputTokens = ref(0);
  const estimatedContextTokens = ref(0);
  /** Context window of the service's primary model; absent when it cannot be resolved. */
  const contextLimitTokens = ref<number | undefined>(undefined);
  const averageCacheHitRate = ref<number | undefined>(undefined);

  const selectedTitle = computed(() => selectedSenderName.value || selectedSenderId.value);
  /** A turn is still generating when it was started but has no finish event yet. */
  const hasActiveTurn = computed(() => turns.value.some((turn) => !turn.finished));
  /** The live window starts at its oldest recorded turn; the archive is not part of it. */
  const contextStartedAt = computed(() => turns.value[0]?.createdAt);
  /** The reply of the newest turn is the last message; a turn still generating uses its start. */
  const lastMessageAt = computed(() => {
    const last = turns.value[turns.value.length - 1];
    if (!last) return undefined;
    const startedAt = new Date(last.createdAt);
    if (Number.isNaN(startedAt.getTime()) || typeof last.generationMs !== "number") {
      return last.createdAt;
    }
    return new Date(startedAt.getTime() + last.generationMs).toISOString();
  });
  const averageTokensPerSecond = computed(() => {
    let sum = 0;
    let count = 0;
    for (const turn of turns.value) {
      if (typeof turn.tokensPerSecond === "number") {
        sum += turn.tokensPerSecond;
        count += 1;
      }
    }
    return count ? sum / count : undefined;
  });

  async function loadUsers() {
    loadingUsers.value = true;
    try {
      const response = await explorer.queryServiceChatUsers(serviceId);
      backend.value = response.backend;
      users.value = response.users;
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      loadingUsers.value = false;
    }
  }

  function resetFlow() {
    turns.value = [];
    archivedTurns.value = [];
    liveChunkEntries.clear();
    inputTokens.value = 0;
    outputTokens.value = 0;
    estimatedContextTokens.value = 0;
    averageCacheHitRate.value = undefined;
  }

  function toTurnView(turn: ServiceChatContextTurn): ContextTurnView {
    const entries: ContextEntryItem[] = [];
    turn.entries.forEach((entry, index) => {
      if (entry.kind === "stage") {
        entries.push({
          kind: "stage",
          id: `${turn.turn_id}:stage:${index}`,
          at: entry.at,
          stage: entry.stage,
          detail: entry.detail,
        });
        return;
      }
      if (entry.kind === "sub_agent") {
        entries.push({
          kind: "sub-agent",
          id: `${turn.turn_id}:sub-agent:${index}`,
          at: entry.at,
          path: entry.path,
          event: entry.event,
        });
        return;
      }
      entries.push({
        kind: "tool",
        id: `${turn.turn_id}:tool:${entry.call_id}`,
        callId: entry.call_id,
        name: entry.name,
        arguments: prettyJson(entry.arguments),
        result: entry.result,
        durationMs: entry.duration_ms,
        running: false,
      });
    });
    return {
      turnId: turn.turn_id,
      createdAt: turn.created_at,
      userText: turn.user_text,
      groupName: turn.group_name,
      quoted: toQuotedView(turn.quoted),
      media: turn.media ?? [],
      entries,
      assistantText: turn.assistant_text,
      reasoningText: turn.reasoning_text,
      usage: turn.usage,
      generationMs: turn.generation_ms,
      tokensPerSecond: turn.output_tokens_per_second,
      continuesPrevious: turn.continues_previous ?? false,
      finished: turn.finished,
    };
  }

  function applySavedContext(response: ServiceChatContextResponse) {
    backend.value = response.backend;
    turns.value = response.turns.map(toTurnView);
    archivedTurns.value = (response.archived ?? []).map(toTurnView);
    // The recorded log holds settled messages only, so any live entry it replaces is done.
    liveChunkEntries.clear();
    inputTokens.value = response.input_tokens;
    outputTokens.value = response.output_tokens;
    estimatedContextTokens.value = response.estimated_context_tokens;
    contextLimitTokens.value = response.context_limit_tokens;
    averageCacheHitRate.value = response.average_cache_hit_rate;
  }

  async function selectUser(user: ServiceChatUserItem) {
    selectedSenderId.value = user.sender_id;
    selectedSenderName.value = user.sender_name;
    resetFlow();
    loadingContext.value = true;
    try {
      applySavedContext(await explorer.queryServiceChatContext(serviceId, user.sender_id));
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      loadingContext.value = false;
    }
  }

  /**
   * Drop the live context window of the selected sender. Finished turns were already archived
   * in the relational database, so the reload shows them under history instead of as current.
   */
  async function clearContext() {
    if (!selectedSenderId.value || clearingContext.value) return;
    const senderId = selectedSenderId.value;
    if (!window.confirm(`确认清除 ${selectedTitle.value} 的当前上下文吗？已完成的轮次会转入历史记录。`)) return;
    clearingContext.value = true;
    try {
      await explorer.clearServiceChatContext(serviceId, senderId);
      resetFlow();
      applySavedContext(await explorer.queryServiceChatContext(serviceId, senderId));
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      clearingContext.value = false;
    }
  }

  function clearSelection() {
    selectedSenderId.value = "";
    selectedSenderName.value = "";
    resetFlow();
  }

  function upsertUser(senderId: string, senderName: string, groupName?: string | null) {
    const existing = users.value.find((user) => user.sender_id === senderId);
    if (existing) {
      existing.sender_name = senderName || existing.sender_name;
      if (groupName) existing.group_name = groupName;
      existing.last_time = new Date().toISOString();
      users.value = [...users.value];
      return;
    }
    users.value = [
      {
        sender_id: senderId,
        sender_name: senderName,
        group_name: groupName ?? undefined,
        context_tokens: 0,
        last_time: new Date().toISOString(),
      },
      ...users.value,
    ];
  }

  function ensureTurn(event: QqChatContextEvent & { turn_id: string }): ContextTurnView {
    const existing = turns.value.find((turn) => turn.turnId === event.turn_id);
    if (existing) return existing;
    const created: ContextTurnView = {
      turnId: event.turn_id,
      createdAt: "created_at" in event ? String(event.created_at) : new Date().toISOString(),
      userText: event.kind === "turn_started" ? event.user_text : "",
      groupName: event.kind === "turn_started" ? event.group_name : undefined,
      quoted: event.kind === "turn_started" ? toQuotedView(event.quoted) : undefined,
      media: event.kind === "turn_started" ? event.media ?? [] : [],
      entries: [],
      continuesPrevious: event.kind === "turn_started" ? event.continues_previous ?? false : false,
      finished: false,
    };
    turns.value = [...turns.value, created];
    return created;
  }

  /** Fold one live WS event into the flow so the view updates in real time. */
  function handleEvent(event: QqChatContextEvent) {
    if (event.agent_id !== serviceId) return;
    if (event.kind === "turn_started") {
      upsertUser(event.sender_id, event.sender_name, event.group_name);
      if (event.sender_id !== selectedSenderId.value) return;
      ensureTurn(event);
      return;
    }
    if (event.sender_id !== selectedSenderId.value) return;
    if (event.kind === "tool_started") {
      const turn = ensureTurn(event);
      turn.entries.push({
        kind: "tool",
        id: `${event.turn_id}:tool:${event.call_id}`,
        callId: event.call_id,
        name: event.tool_name,
        arguments: prettyJson(event.arguments),
        running: true,
      });
      return;
    }
    if (event.kind === "tool_finished") {
      const turn = ensureTurn(event);
      const tool = turn.entries.find(
        (entry): entry is ContextToolItem => entry.kind === "tool" && entry.callId === event.call_id,
      );
      if (tool) {
        tool.running = false;
        tool.result = event.result;
        tool.durationMs = event.duration_ms;
      }
      return;
    }
    if (event.kind === "stage") {
      const turn = ensureTurn(event);
      turn.entries.push({
        kind: "stage",
        id: `${event.turn_id}:stage:${turn.entries.length}`,
        at: event.at,
        stage: event.stage,
        detail: event.detail,
      });
      return;
    }
    if (event.kind === "sub_agent") {
      const turn = ensureTurn(event);
      // Several sub-agents may stream at once, so each partly written message gets its own entry
      // and is looked up by call path instead of by position.
      const liveKey = `${event.turn_id}\u0000${event.path.join("\u0000")}`;
      if (event.event.kind === "assistant_chunk") {
        const existing = liveChunkEntries.get(liveKey);
        if (existing && existing.event.kind === "assistant_chunk") {
          existing.event = {
            kind: "assistant_chunk",
            content: existing.event.content + event.event.content,
            reasoning: existing.event.reasoning + event.event.reasoning,
          };
          return;
        }
        const entry: ContextSubAgentItem = {
          kind: "sub-agent",
          id: `${event.turn_id}:sub-agent:${turn.entries.length}`,
          at: event.at,
          path: event.path,
          event: event.event,
        };
        turn.entries.push(entry);
        // Keep the reactive element, not the raw object pushed: later deltas mutate it in
        // place, and only the proxy's writes re-render the flow.
        liveChunkEntries.set(liveKey, turn.entries[turn.entries.length - 1] as ContextSubAgentItem);
        return;
      }
      // The settled message replaces the live text; the next response streams into a new entry.
      liveChunkEntries.delete(liveKey);
      turn.entries.push({
        kind: "sub-agent",
        id: `${event.turn_id}:sub-agent:${turn.entries.length}`,
        at: event.at,
        path: event.path,
        event: event.event,
      });
      return;
    }
    if (event.kind === "turn_finished") {
      const turn = ensureTurn(event);
      turn.assistantText = event.assistant_text;
      turn.reasoningText = event.reasoning_text;
      turn.usage = event.usage;
      turn.generationMs = event.generation_ms;
      turn.tokensPerSecond = event.output_tokens_per_second;
      turn.finished = true;
      if (event.usage) {
        inputTokens.value += event.usage.prompt_tokens ?? 0;
        outputTokens.value += event.usage.completion_tokens ?? 0;
      }
      // Reconcile with the recorded log (also refreshes totals and tool durations).
      void explorer
        .queryServiceChatContext(serviceId, event.sender_id)
        .then(applySavedContext)
        .catch(() => undefined);
    }
  }

  const unsubscribe = ws.onMessage((message) => {
    if (message.type === "QqContext") handleEvent(message.event);
  });
  onUnmounted(unsubscribe);

  return {
    backend,
    loadingUsers,
    users,
    loadingContext,
    clearingContext,
    selectedSenderId,
    selectedSenderName,
    selectedTitle,
    turns,
    archivedTurns,
    inputTokens,
    outputTokens,
    estimatedContextTokens,
    contextLimitTokens,
    contextStartedAt,
    lastMessageAt,
    hasActiveTurn,
    averageCacheHitRate,
    averageTokensPerSecond,
    loadUsers,
    selectUser,
    clearSelection,
    clearContext,
  };
}

function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2) ?? "";
  } catch {
    return String(value ?? "");
  }
}

function toQuotedView(quoted?: ServiceChatContextQuoted | QqContextQuoted): ContextQuotedMessage | undefined {
  if (!quoted) return undefined;
  return {
    messageId: quoted.message_id,
    senderName: quoted.sender_name,
    senderId: quoted.sender_id,
    text: quoted.text,
  };
}
