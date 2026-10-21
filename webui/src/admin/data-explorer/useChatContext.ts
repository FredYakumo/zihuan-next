import { computed, onUnmounted, ref } from "vue";

import {
  explorer,
  type ServiceChatContextMedia,
  type ServiceChatContextQuoted,
  type ServiceChatContextResponse,
  type ServiceChatContextTurn,
  type ServiceChatUserItem,
} from "../../api/client";
import type { QqChatContextEvent, QqContextQuoted, TokenUsage } from "../../api/types";
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

/** A stage or tool call, kept in the order it happened so the flow reads chronologically. */
export type ContextEntryItem = ContextStageItem | ContextToolItem;

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
  const turns = ref<ContextTurnView[]>([]);
  /** Turns restored from the relational archive; shown above a divider, before the live window. */
  const archivedTurns = ref<ContextTurnView[]>([]);
  const inputTokens = ref(0);
  const outputTokens = ref(0);
  const estimatedContextTokens = ref(0);
  const averageCacheHitRate = ref<number | undefined>(undefined);

  const selectedTitle = computed(() => selectedSenderName.value || selectedSenderId.value);
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
    inputTokens.value = response.input_tokens;
    outputTokens.value = response.output_tokens;
    estimatedContextTokens.value = response.estimated_context_tokens;
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
        turn_count: 0,
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
    selectedSenderId,
    selectedSenderName,
    selectedTitle,
    turns,
    archivedTurns,
    inputTokens,
    outputTokens,
    estimatedContextTokens,
    averageCacheHitRate,
    averageTokensPerSecond,
    loadUsers,
    selectUser,
    clearSelection,
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
