import { computed, onUnmounted, ref, watch } from "vue";

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
  /** Backend id of a stage that updates in place (for example an embedding start → finish). */
  stageId?: string;
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
  stepsAvailable?: boolean;
  entriesLoading?: boolean;
  entriesError?: string;
  groupId?: string;
  isGroup?: boolean;
  senderName?: string;
  status?: string;
  turnId: string;
  createdAt: string;
  userText: string;
  /** Group the user message was sent in; only present for group chats. */
  groupName?: string;
  /** Quoted message shown as a collapsible block inside the user entry. */
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
  const usersTotal = ref(0);
  const usersPage = ref(1);
  const usersQuery = ref("");
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  const historyCursor = ref<string | undefined>();
  const loadingHistory = ref(false);
  const compacted = ref(false);
  const associationIncomplete = ref(false);
  let windowVersion = -1;
  let usersRequest = 0;
  let eventRevision = 0;
  const users = ref<ServiceChatUserItem[]>([]);
  const selectedSenderId = ref("");
  const selectedSenderName = ref("");

  const loadingContext = ref(false);
  const clearingContext = ref(false);
  /** Turn whose record is currently being deleted, so its delete control can show progress. */
  const deletingTurnId = ref("");
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

  async function loadUsers(page = usersPage.value) {
    const request = ++usersRequest;
    const query = usersQuery.value.trim();
    usersPage.value = page;
    loadingUsers.value = true;
    try {
      const response = await explorer.queryServiceChatUsers(serviceId, page, query);
      if (request !== usersRequest || query !== usersQuery.value.trim()) return;
      const lastPage = Math.max(1, Math.ceil(response.total / response.page_size));
      if (page > lastPage) { await loadUsers(lastPage); return; }
      backend.value = response.backend;
      usersTotal.value = response.total;
      usersPage.value = response.page;
      users.value = response.users;
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      if (request === usersRequest) loadingUsers.value = false;
    }
  }

  function searchUsers() {
    clearTimeout(searchTimer);
    void loadUsers(1);
  }

  function refreshUsers() {
    clearTimeout(searchTimer);
    void loadUsers();
  }

  watch(usersQuery, () => {
    clearTimeout(searchTimer);
    usersPage.value = 1;
    searchTimer = setTimeout(searchUsers, 300);
  }, { flush: "sync" });

  async function refreshContext() {
    const sender = selectedSenderId.value;
    if (!sender) return;
    const revision = eventRevision;
    const response = await explorer.queryServiceChatContext(serviceId, sender);
    if (sender !== selectedSenderId.value) return;
    if (revision !== eventRevision) { void refreshContext(); return; }
    applySavedContext(response);
  }

  async function loadHistory() {
    if (!historyCursor.value || loadingHistory.value) return;
    const sender = selectedSenderId.value;
    loadingHistory.value = true;
    try {
      const response = await explorer.queryServiceChatContext(serviceId, sender, historyCursor.value);
      if (sender !== selectedSenderId.value || response.window_version !== windowVersion) return;
      const currentIds = new Set(response.current_turn_ids);
      const merged = new Map([...response.archived.map(toTurnView), ...archivedTurns.value].map((turn) => [turn.turnId, turn]));
      archivedTurns.value = [...merged.values()].filter((turn) => !currentIds.has(turn.turnId)).sort((a, b) => a.createdAt.localeCompare(b.createdAt) || a.turnId.localeCompare(b.turnId));
      historyCursor.value = response.next_cursor ?? undefined;
    } catch (error) { alert((error as Error).message); }
    finally { loadingHistory.value = false; }
  }

  function resetFlow() {
    turns.value = [];
    archivedTurns.value = [];
    historyCursor.value = undefined;
    windowVersion = -1;
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
          stageId: entry.id,
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
      stepsAvailable: turn.steps_available,
      groupId: turn.group_id,
      isGroup: turn.is_group,
      senderName: turn.sender_name,
      status: turn.status,
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
    if (response.window_version < windowVersion) return;
    windowVersion = response.window_version;
    historyCursor.value = response.next_cursor ?? undefined;
    compacted.value = response.compacted;
    associationIncomplete.value = response.association_incomplete;
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
      await refreshContext();
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
      if (senderId !== selectedSenderId.value) return;
      resetFlow();
      await refreshContext();
      void loadUsers();
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      clearingContext.value = false;
    }
  }

  /**
   * Delete one conversation turn's record: the backend drops the process turn log entry and the
   * archived row, then the flow re-reads the context so the turn disappears from the view.
   */
  async function deleteTurn(turn: ContextTurnView) {
    if (!selectedSenderId.value || deletingTurnId.value) return;
    const title = turn.userText.length > 24 ? `${turn.userText.slice(0, 24)}…` : turn.userText;
    if (!window.confirm(`确认删除这条对话记录吗？\n「${title || turn.createdAt}」`)) return;
    deletingTurnId.value = turn.turnId;
    try {
      const senderId = selectedSenderId.value;
      await explorer.deleteServiceChatTurn(serviceId, senderId, turn.turnId);
      // Replace the view in place; resetting to an empty flow first would flash the empty state.
      if (senderId === selectedSenderId.value) await refreshContext();
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      deletingTurnId.value = "";
    }
  }

  function clearSelection() {
    selectedSenderId.value = "";
    selectedSenderName.value = "";
    resetFlow();
  }

  function ensureTurn(event: QqChatContextEvent & { turn_id: string }): ContextTurnView {
    const existing = turns.value.find((turn) => turn.turnId === event.turn_id);
    if (existing) return existing;
    const created: ContextTurnView = {
      turnId: event.turn_id,
      createdAt: "created_at" in event ? String(event.created_at) : new Date().toISOString(),
      userText: event.kind === "turn_started" ? event.user_text : "",
      groupName: event.kind === "turn_started" ? event.group_name : undefined,
      groupId: event.kind === "turn_started" ? event.group_id : undefined,
      isGroup: event.kind === "turn_started" ? event.is_group : undefined,
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
    eventRevision += 1;
    if (event.kind === "turn_started") {
      void loadUsers();
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
      // A progress stage reports its start and finish under the same backend id. Keep the
      // existing entry and only replace its detail, so the entry updates instead of repeating.
      if (event.id) {
        const existing = turn.entries.find(
          (entry): entry is ContextStageItem =>
            entry.kind === "stage" && entry.stageId === event.id,
        );
        if (existing) {
          existing.detail = event.detail;
          return;
        }
      }
      turn.entries.push({
        kind: "stage",
        id: `${event.turn_id}:stage:${turn.entries.length}`,
        stageId: event.id,
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
      void refreshContext().catch(() => undefined);
      void loadUsers();
    }
  }

  let streamId = "";
  const eventVersions = new Map<string, number>();
  const unsubscribe = ws.onMessage((message) => {
    if (message.type !== "QqContext" || message.event.agent_id !== serviceId) return;
    if (message.stream_id !== streamId) { streamId = message.stream_id; eventVersions.clear(); }
    if ((eventVersions.get(message.event_id) ?? -1) >= message.revision) return;
    eventVersions.set(message.event_id, message.revision);
    if (eventVersions.size > 4096) eventVersions.delete(eventVersions.keys().next().value!);
    handleEvent(message.event);
  });
  const unsubscribeReconnect = ws.onReconnect(() => { void loadUsers(); void refreshContext().catch(() => undefined); });
  onUnmounted(() => { clearTimeout(searchTimer); usersRequest += 1; unsubscribe(); unsubscribeReconnect(); });

  return {
    backend,
    loadingUsers,
    usersTotal, usersPage, usersQuery, searchUsers, refreshUsers, historyCursor, loadingHistory, loadHistory, compacted, associationIncomplete,
    users,
    loadingContext,
    clearingContext,
    deletingTurnId,
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
    deleteTurn,
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
