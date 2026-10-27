import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import type { ServiceChatContextMedia } from "../../api/client";
import type { QqSubAgentEntry, TokenUsage } from "../../api/types";
import type {
  ContextEntryItem,
  ContextStageItem,
  ContextToolItem,
  ContextTurnView,
} from "./useChatContext";

type Segment = { type: "text"; text: string } | { type: "image"; media: ServiceChatContextMedia };

/** Stage name the backend uses for the per-turn assembled system prompt. */
export const SYSTEM_PROMPT_STAGE = "System Prompt";

/** Stage reporting the memory titles retrieved for the turn before the brain runs. */
export const MEMORY_STAGE = "记忆检索";

/** Stage reporting the assembled current-turn user input. */
export const USER_INPUT_STAGE = "User Input";

/** Stage the QQ Chat runtime emits with the rate-limit decision of the turn. */
export const RATE_LIMIT_STAGE = "限流";

/** Stage the local embedding runtime emits while one embedding inference runs. */
export const EMBEDDING_STAGE = "嵌入推理";

/**
 * One stage of the turn, wrapping the individual steps it consists of.
 *
 * Stages follow `documents/design/role/qq_chat_role.md`, so the flow reads in the same order
 * as the documented pipeline.
 */
export interface ContextPhase {
  key: string;
  name: string;
  /** Stage and tool steps of the phase; sub-agent steps render inside their call's box. */
  entries: Array<ContextStageItem | ContextToolItem>;
}

/**
 * Which stage each reported step belongs to. Stages the runtime reports under their own name
 * still form their own container, so this only needs the steps that belong together.
 */
const PHASE_OF_STAGE: Record<string, string> = {
  [RATE_LIMIT_STAGE]: "触发与限流",
  "意图识别": "意图识别",
  [SYSTEM_PROMPT_STAGE]: "构建 Brain 输入",
  [MEMORY_STAGE]: "构建 Brain 输入",
  "情绪与风格提示": "构建 Brain 输入",
  [USER_INPUT_STAGE]: "构建 Brain 输入",
  "Brain 输入": "构建 Brain 输入",
  "推理上下文": "构建 Brain 输入",
  "回复决策": "Brain 推理",
  "Rethink": "回复Review",
  "回复审查与改写": "回复Review",
  "发送回复": "发送回复",
  "元查询直答": "元查询直答",
  "错误": "错误",
};

/** Phase a step belongs to; tool calls all happen inside the brain's inference loop. */
const BRAIN_PHASE = "Brain 推理";

/** Adjacent phases use different entries of this palette. */
export const PHASE_COLORS = ["#4d8dff", "#34c759", "#ff9f0a", "#a06bff", "#ff5d5d", "#14b8a6"];

export function phaseColor(index: number): string {
  return PHASE_COLORS[index % PHASE_COLORS.length];
}

/**
 * Group one turn's steps into consecutive stages.
 *
 * Grouping is sequential rather than by name: a stage that reappears after another one (for
 * example the error report after a failed step) starts a new container instead of being merged
 * backwards. Sub-agent steps are not stages of the turn; they render inside the box attached to
 * the tool call that started them.
 */
export function groupEntriesByPhase(entries: ContextEntryItem[]): ContextPhase[] {
  const phases: ContextPhase[] = [];
  for (const entry of entries) {
    if (entry.kind === "sub-agent") continue;
    const name =
      entry.kind === "tool" ? BRAIN_PHASE : (PHASE_OF_STAGE[entry.stage] ?? entry.stage);
    const last = phases[phases.length - 1];
    if (last && last.name === name) {
      last.entries.push(entry);
      continue;
    }
    phases.push({ key: `${entry.id}:phase`, name, entries: [entry] });
  }
  return phases;
}

/** One tool call inside a sub-agent's loop, pairing its start event with its result. */
export interface SubAgentToolCall {
  callId: string;
  name: string;
  arguments: unknown;
  result?: string;
  durationMs?: number;
  running: boolean;
}

/** A sub-agent assistant message, as reported by its own loop. */
export type SubAgentMessage = Extract<QqSubAgentEntry, { kind: "assistant" }>;

/** One step of a sub-agent's loop, kept in the order it happened. */
export type SubAgentStep =
  | { kind: "assistant"; message: SubAgentMessage }
  | { kind: "tool"; call: SubAgentToolCall };

/** The response currently being streamed by a sub-agent. */
export interface SubAgentStream {
  content: string;
  reasoning: string;
}

/** A sub-agent invocation and everything its own loop did, nested under its tool call. */
export interface SubAgentNode {
  key: string;
  callId: string;
  /** 1 for a sub-agent called by the turn's brain; deeper values are nested sub-agents. */
  depth: number;
  name: string;
  /** Assistant messages and tool calls of the sub-agent's loop, in chronological order. */
  steps: SubAgentStep[];
  /** Every tool call this sub-agent made, in order; the same objects `steps` references. */
  toolCalls: SubAgentToolCall[];
  /** Text streamed so far for the response still being written; settled messages replace it. */
  live?: SubAgentStream;
  /** The prompt the sub-agent received. */
  input?: Extract<QqSubAgentEntry, { kind: "input" }>;
  children: SubAgentNode[];
}

/** Assistant messages of a sub-agent, in order. */
export function subAgentMessages(node: SubAgentNode): SubAgentMessage[] {
  return node.steps.flatMap((step) => (step.kind === "assistant" ? [step.message] : []));
}

/**
 * Rebuilds the sub-agent call tree from the flat, chronological sub-agent entries.
 *
 * Every entry carries its enclosing call ids, so the tree is derived from those paths rather
 * than from arrival order: a nested sub-agent always lands under the call that started it,
 * even while its parent is still running.
 */
export function buildSubAgentTrees(entries: ContextEntryItem[]): Map<string, SubAgentNode> {
  // Names of every tool call: top-level calls from the turn's tool entries, and calls made by
  // sub-agents from their own start events. A nested sub-agent is addressed by the id of the
  // call that started it, so both sources are needed to label the boxes.
  const toolNames = new Map<string, string>();
  for (const entry of entries) {
    if (entry.kind === "tool") {
      toolNames.set(entry.callId, entry.name);
    } else if (entry.kind === "sub-agent" && entry.event.kind === "tool_started") {
      toolNames.set(entry.event.call_id, entry.event.name);
    }
  }

  const nodes = new Map<string, SubAgentNode>();
  const ensureNode = (path: string[], parent?: SubAgentNode): SubAgentNode => {
    const key = path.join("\u0000");
    const existing = nodes.get(key);
    if (existing) return existing;
    const callId = path[path.length - 1];
    const node: SubAgentNode = {
      key,
      callId,
      depth: path.length,
      name: toolNames.get(callId) ?? callId,
      steps: [],
      toolCalls: [],
      children: [],
    };
    nodes.set(key, node);
    parent?.children.push(node);
    return node;
  };

  const toolCalls = new Map<string, SubAgentToolCall>();
  for (const entry of entries) {
    if (entry.kind !== "sub-agent" || !entry.path.length) continue;
    let parent: SubAgentNode | undefined;
    let node: SubAgentNode | undefined;
    for (let index = 0; index < entry.path.length; index += 1) {
      node = ensureNode(entry.path.slice(0, index + 1), parent);
      parent = node;
    }
    if (!node) continue;
    const event = entry.event;
    if (event.kind === "input") {
      node.input = event;
    } else if (event.kind === "assistant_chunk") {
      // A delta starts the live block; later deltas of the same response append to it.
      const live = node.live ?? { content: "", reasoning: "" };
      node.live = {
        content: live.content + event.content,
        reasoning: live.reasoning + event.reasoning,
      };
    } else if (event.kind === "assistant") {
      // The settled message replaces the deltas that built it. Responses with neither text nor
      // reasoning only opened the tool call that follows, so they add no step.
      node.live = undefined;
      if (event.content.trim() || event.reasoning) {
        node.steps.push({ kind: "assistant", message: event });
      }
    } else if (event.kind === "tool_started") {
      const call: SubAgentToolCall = {
        callId: event.call_id,
        name: event.name,
        arguments: event.arguments,
        running: true,
      };
      node.toolCalls.push(call);
      node.steps.push({ kind: "tool", call });
      toolCalls.set(`${node.key}\u0000${event.call_id}`, call);
    } else {
      const call = toolCalls.get(`${node.key}\u0000${event.call_id}`);
      if (call) {
        call.running = false;
        call.result = event.result;
        call.durationMs = event.duration_ms;
      } else {
        // A result without its start event (truncated log): show it as its own step.
        const orphan: SubAgentToolCall = {
          callId: event.call_id,
          name: event.name,
          arguments: undefined,
          result: event.result,
          durationMs: event.duration_ms,
          running: false,
        };
        node.toolCalls.push(orphan);
        node.steps.push({ kind: "tool", call: orphan });
      }
    }
  }
  return nodes;
}

interface RateLimitStageDetail {
  configured?: boolean;
  unlimited?: boolean;
  scope_type?: string | null;
  window_unit?: string | null;
  window_size?: number | null;
  max_calls?: number | null;
  used_calls?: number | null;
  warning_level?: "warning" | "severe" | null;
}

function rateLimitDetail(stage: string, detail: unknown): RateLimitStageDetail | undefined {
  if (stage !== RATE_LIMIT_STAGE) return undefined;
  if (!detail || typeof detail !== "object") return undefined;
  return detail as RateLimitStageDetail;
}

const WINDOW_UNIT_LABELS: Record<string, string> = { minute: "分钟", hour: "小时", day: "天" };
const SCOPE_LABELS: Record<string, string> = { user: "用户规则", group: "群组规则", default: "默认规则" };

/** One-line summary of the quota that applied to a turn, e.g. `用户规则 · 每小时 5 次 · 已用 3/5`. */
export function rateLimitText(stage: string, detail: unknown): string | undefined {
  const parsed = rateLimitDetail(stage, detail);
  if (!parsed) return undefined;
  const scope = parsed.scope_type ? SCOPE_LABELS[parsed.scope_type] : undefined;
  if (!parsed.configured || parsed.unlimited) {
    return scope ? `${scope} · 无限制` : "无限制";
  }

  const parts: string[] = [];
  if (scope) parts.push(scope);
  const unit = parsed.window_unit ? WINDOW_UNIT_LABELS[parsed.window_unit] : undefined;
  const size = parsed.window_size ?? 1;
  if (unit && parsed.max_calls != null) {
    parts.push(size === 1 ? `每${unit} ${parsed.max_calls} 次` : `每 ${size} ${unit} ${parsed.max_calls} 次`);
  }
  if (parsed.max_calls != null) {
    parts.push(`已用 ${parsed.used_calls ?? 0}/${parsed.max_calls}`);
  }
  return parts.length ? parts.join(" · ") : "无限制";
}

/** Tone for the rate-limit bubble: the runtime warns as the quota runs out. */
export function rateLimitSeverity(stage: string, detail: unknown): "warning" | "danger" | undefined {
  const parsed = rateLimitDetail(stage, detail);
  if (!parsed) return undefined;
  if (parsed.warning_level === "severe") return "danger";
  if (parsed.warning_level === "warning") return "warning";
  return undefined;
}

/** Text a stage produced or received, rendered as readable prose instead of raw JSON. */
export interface StageText {
  /** Prose the stage carries, when it has any. An embedding stage has none. */
  text?: string;
  /** One-line summary of what produced the text (model, duration, token counts). */
  meta?: string;
  /** The model's thinking for this stage, when the stage recorded it. */
  reasoning?: string;
  /** History messages the stage sent along with `text`, oldest first. */
  history?: StageHistoryMessage[];
  /** Live facts of an embedding inference, when this stage reports one. */
  embedding?: EmbeddingStageView;
}

export interface StageHistoryMessage {
  role: string;
  chars: number;
  preview: string;
}

/** One label/value fact of an embedding inference. */
export interface EmbeddingStageField {
  label: string;
  value: string;
}

/** Readable view of one embedding inference report, rendered as its own bubble. */
export interface EmbeddingStageView {
  status: "running" | "done" | "error";
  /** Status line under the bubble title, with the live elapsed time while it runs. */
  statusText: string;
  fields: EmbeddingStageField[];
  preview?: string;
  error?: string;
}

function detailRecord(detail: unknown): Record<string, unknown> | undefined {
  return detail && typeof detail === "object" ? (detail as Record<string, unknown>) : undefined;
}

function detailText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const minutes = Math.floor(ms / 60_000);
  return `${minutes} 分 ${((ms % 60_000) / 1000).toFixed(1)} 秒`;
}

export function embeddingStatus(detail: unknown): "running" | "done" | "error" {
  const status = detailRecord(detail)?.status;
  return status === "done" || status === "error" ? status : "running";
}

/**
 * Readable view of an embedding inference.
 *
 * The backend emits the same stage twice: a `running` report when inference starts (model,
 * backend, device, input size) and a finished/error report with the measured duration and the
 * output shape. While it runs, the elapsed time is derived from `started_at` so it keeps ticking
 * between events; `now` is the shared clock of the turn view.
 */
export function embeddingStageView(detail: unknown, now: number): EmbeddingStageView | undefined {
  const record = detailRecord(detail);
  if (!record) return undefined;

  const status = embeddingStatus(detail);
  const startedAt =
    typeof record.started_at === "string" ? Date.parse(record.started_at) : Number.NaN;
  const elapsedMs =
    status === "running" && !Number.isNaN(startedAt) ? Math.max(0, now - startedAt) : undefined;
  const durationMs = typeof record.duration_ms === "number" ? record.duration_ms : undefined;
  const statusText =
    status === "running"
      ? elapsedMs !== undefined
        ? `推理中…（已等待 ${formatDuration(elapsedMs)}）`
        : "推理中…"
      : status === "error"
        ? durationMs !== undefined
          ? `推理失败（耗时 ${formatDuration(durationMs)}）`
          : "推理失败"
        : `推理完成（耗时 ${formatDuration(durationMs ?? 0)}）`;

  const fields: EmbeddingStageField[] = [];
  if (typeof record.model === "string") fields.push({ label: "模型", value: record.model });
  if (typeof record.backend === "string") fields.push({ label: "后端", value: record.backend });
  if (typeof record.device === "string") fields.push({ label: "推理设备", value: record.device });
  if (typeof record.input_count === "number") {
    const chars = typeof record.input_chars === "number" ? ` · ${record.input_chars} 字` : "";
    fields.push({ label: "输入", value: `${record.input_count} 条${chars}` });
  }
  if (Array.isArray(record.shape)) {
    fields.push({ label: "输出 shape", value: `[${record.shape.join(", ")}]` });
  } else if (
    status === "running" &&
    typeof record.dimension === "number" &&
    typeof record.input_count === "number"
  ) {
    fields.push({
      label: "输出 shape",
      value: `[${record.input_count}, ${record.dimension}]（进行中）`,
    });
  }
  if (typeof record.instance_id === "string") {
    fields.push({ label: "实例", value: record.instance_id });
  }

  return {
    status,
    statusText,
    fields,
    preview: detailText(record.preview),
    error: detailText(record.error),
  };
}

/**
 * Readable view of a stage's payload.
 *
 * The prompt-building stages carry one long text each: the assembled system prompt, the
 * retrieved memory titles, and the current-turn input handed to the brain. Showing them as text
 * is the point of the flow view; everything else stays JSON.
 */
export function stageText(
  stage: string | undefined,
  detail: unknown,
  now = Date.now(),
): StageText | undefined {
  if (!stage) return undefined;
  if (stage === EMBEDDING_STAGE) {
    const embedding = embeddingStageView(detail, now);
    return embedding ? { embedding } : undefined;
  }
  if (typeof detail === "string") {
    return stage === SYSTEM_PROMPT_STAGE ? { text: detail } : undefined;
  }
  const record = detailRecord(detail);
  if (!record) return undefined;

  if (stage === SYSTEM_PROMPT_STAGE) {
    const prompt = detailText(record.prompt);
    return prompt ? { text: prompt } : undefined;
  }

  // The retrieval store's answer to the current message: query plus the titles above the
  // relevance threshold. The hint tells the brain to search missing memories with memory_agent.
  if (stage === MEMORY_STAGE) {
    const query = detailText(record.query);
    const titles = Array.isArray(record.titles)
      ? record.titles.flatMap((item): Array<{ title: string; relevance?: number }> => {
          const entry = detailRecord(item);
          if (!entry || typeof entry.title !== "string") return [];
          return [{
            title: entry.title,
            relevance: typeof entry.relevance === "number" ? entry.relevance : undefined,
          }];
        })
      : [];
    const lines: string[] = [];
    if (query) lines.push(`检索内容：${query}`);
    if (titles.length) {
      lines.push("关联度较高的记忆标题：");
      for (const item of titles) {
        lines.push(
          item.relevance != null
            ? `- ${item.title}（${item.relevance.toFixed(2)}）`
            : `- ${item.title}`,
        );
      }
    } else {
      lines.push("没有关联度较高的记忆标题，Agent 会改用 memory_agent 检索缺失内容。");
    }
    const meta: string[] = [];
    if (typeof record.candidates === "number") meta.push(`候选 ${record.candidates} 条`);
    if (typeof record.min_relevance === "number") meta.push(`相关度阈值 ${record.min_relevance}`);
    if (typeof record.top_n === "number") meta.push(`最多 ${record.top_n} 条`);
    return { text: lines.join("\n"), meta: meta.join(" · ") };
  }

  if (stage === "Brain 输入") {
    const text = detailText(record.current_user_message);
    if (!text) return undefined;
    const meta: string[] = [];
    if (typeof record.model === "string") meta.push(record.model);
    if (typeof record.messages_sent === "number") meta.push(`共 ${record.messages_sent} 条消息`);
    if (typeof record.prompt_tokens_estimated === "number") {
      meta.push(`预估 ${record.prompt_tokens_estimated} tokens`);
    }
    if (record.compacted === true) meta.push("历史已压缩");
    const history = Array.isArray(record.history)
      ? record.history.flatMap((item): StageHistoryMessage[] => {
          const entry = detailRecord(item);
          if (!entry || typeof entry.role !== "string") return [];
          return [
            {
              role: entry.role,
              chars: typeof entry.chars === "number" ? entry.chars : 0,
              preview: typeof entry.preview === "string" ? entry.preview : "",
            },
          ];
        })
      : [];
    return { text, meta: meta.join(" · "), history };
  }

  return undefined;
}

export function useChatContextTurn(turn: () => ContextTurnView) {
  const rootEl = ref<HTMLElement | null>(null);
  const previewMedia = ref<ServiceChatContextMedia | null>(null);
  // Rich view replaces `[Image: media_id=...]` placeholders with the actual images; the original
  // view shows the recorded user text exactly as the agent received it.
  const richView = ref(true);
  const failedMedia = ref(new Set<string>());
  // Reasoning is the model's own explanation of the turn, so it starts visible instead of
  // hidden behind a toggle; it can still be folded away.
  const thinkingOpen = ref(true);
  // The quoted message starts expanded so the user sees what was replied to.
  const quoteOpen = ref(true);

  function toggleQuote() {
    quoteOpen.value = !quoteOpen.value;
  }

  // Per-entry reasoning toggles for stages, kept in the same shape as the assistant's
  // "思考过程" block so both read the same way. Entries start open.
  const stageThinkingClosed = ref(new Set<string>());

  function stageThinkingOpen(id: string): boolean {
    return !stageThinkingClosed.value.has(id);
  }

  function toggleStageThinking(id: string) {
    const next = new Set(stageThinkingClosed.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    stageThinkingClosed.value = next;
  }

  /** `名字(id)` for the quoted message's sender, falling back to whichever half is known. */
  const quoteSender = computed(() => {
    const quoted = turn().quoted;
    if (!quoted) return "";
    const name = quoted.senderName?.trim();
    const id = quoted.senderId?.trim();
    if (name && id) return `${name}(${id})`;
    return name || id || "";
  });
  // Matches the placeholder the message renderer writes for an image, e.g.
  // `[Image: media_id=2026-...]`; the whole placeholder is replaced by the image itself.
  const mediaIdPattern = /\[Image:?\s+media_id=([^\s\]]+)\]/g;

  /** Steps of this turn grouped into stages, each with its own palette color. */
  const phases = computed(() =>
    groupEntriesByPhase(turn().entries).map((phase, index) => ({
      ...phase,
      color: phaseColor(index),
    })),
  );

  /** Sub-agent call trees, keyed by the joined call path of each sub-agent invocation. */
  const subAgentTrees = computed(() => buildSubAgentTrees(turn().entries));

  /** The sub-agent node a top-level tool call started, if this call is a sub-agent. */
  function subAgentsFor(callId: string): SubAgentNode[] {
    const node = subAgentTrees.value.get(callId);
    return node && node.depth === 1 ? [node] : [];
  }

  /**
   * Top-level sub-agent flows whose enclosing tool call is not part of the turn (an archive
   * recorded without it, or a truncated log). They still render, below the stages, so no step
   * is lost.
   */
  const orphanSubAgents = computed(() => {
    const callIds = new Set<string>();
    for (const entry of turn().entries) {
      if (entry.kind === "tool") callIds.add(entry.callId);
    }
    return [...subAgentTrees.value.values()].filter(
      (node) => node.depth === 1 && !callIds.has(node.callId),
    );
  });

  /**
   * Clock shared by embedding bubbles. It only ticks while an embedding inference is running,
   * so settled turns do not keep timers alive.
   */
  const now = ref(Date.now());
  let clockTimer: number | undefined;

  function startClock() {
    if (clockTimer !== undefined) return;
    now.value = Date.now();
    clockTimer = window.setInterval(() => {
      now.value = Date.now();
    }, 1000);
  }

  function stopClock() {
    if (clockTimer === undefined) return;
    window.clearInterval(clockTimer);
    clockTimer = undefined;
  }

  const hasRunningEmbedding = computed(() =>
    turn().entries.some(
      (entry) =>
        entry.kind === "stage" &&
        entry.stage === EMBEDDING_STAGE &&
        embeddingStatus(entry.detail) === "running",
    ),
  );
  watch(hasRunningEmbedding, (running) => (running ? startClock() : stopClock()), {
    immediate: true,
  });

  /** Readable text for each stage entry, keyed by entry id. */
  const stageViews = computed(() => {
    const views = new Map<string, StageText | undefined>();
    for (const entry of turn().entries) {
      if (entry.kind !== "stage") continue;
      views.set(entry.id, stageText(entry.stage, entry.detail, now.value));
    }
    return views;
  });

  /** Interleave the user text with the images it references via `media_id=<id>`. */
  const segments = computed<Segment[]>(() => {
    const currentTurn = turn();
    const text = currentTurn.userText;
    const available = (currentTurn.media ?? []).filter(
      (entry) => entry.url && !failedMedia.value.has(entry.media_id),
    );
    if (!available.length) {
      return text ? [{ type: "text", text }] : [];
    }
    const byId = new Map(available.map((entry) => [entry.media_id, entry]));
    const result: Segment[] = [];
    let cursor = 0;
    for (const match of text.matchAll(mediaIdPattern)) {
      const start = match.index ?? 0;
      if (start > cursor) result.push({ type: "text", text: text.slice(cursor, start) });
      const entry = byId.get(match[1]);
      if (entry) result.push({ type: "image", media: entry });
      else result.push({ type: "text", text: match[0] });
      cursor = start + match[0].length;
    }
    if (cursor < text.length) result.push({ type: "text", text: text.slice(cursor) });
    return result;
  });

  function cacheHit(usage: TokenUsage): number | undefined {
    if (!usage.cached_prompt_tokens || !usage.prompt_tokens) return undefined;
    return usage.cached_prompt_tokens / usage.prompt_tokens;
  }

  function formatPercent(value?: number): string {
    return value === undefined ? "—" : `${(value * 100).toFixed(1)}%`;
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatTime(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function hideImage(media: ServiceChatContextMedia) {
    const next = new Set(failedMedia.value);
    next.add(media.media_id);
    failedMedia.value = next;
    if (previewMedia.value?.media_id === media.media_id) closePreview();
  }

  function toggleRichView() {
    richView.value = !richView.value;
    closePreview();
  }

  function openPreview(media: ServiceChatContextMedia) {
    if (media.url) previewMedia.value = media;
  }

  function closePreview() {
    previewMedia.value = null;
  }

  // The phase currently under the top of the scroll viewport, shown as a floating badge while
  // the user browses the flow. Each turn only reports its own phases, so at most one badge is
  // visible at a time.
  const activePhase = ref<{ name: string; color: string } | null>(null);
  const badgeStyle = ref<Record<string, string>>({});
  const phaseElements = new Map<string, HTMLElement>();
  let scrollRoot: HTMLElement | null = null;
  let frame = 0;

  function registerPhase(key: string, el: Element | null) {
    if (el instanceof HTMLElement) phaseElements.set(key, el);
    else phaseElements.delete(key);
  }

  function updateActivePhase() {
    frame = 0;
    if (!phaseElements.size) return;
    const rootRect = scrollRoot?.getBoundingClientRect();
    const turnRect = rootEl.value?.getBoundingClientRect();
    if (!rootRect || !turnRect) {
      activePhase.value = null;
      return;
    }
    // Every turn renders its own badge at the same fixed spot, so only the turn holding the
    // middle of the viewport may show one; otherwise several badges stack and the wrong turn's
    // name wins.
    const focus = rootRect.top + rootRect.height / 2;
    if (focus < turnRect.top || focus > turnRect.bottom) {
      activePhase.value = null;
      return;
    }
    // Within that turn, the phase occupying the most visible height is the one being read; a
    // phase peeking in at an edge should not win over the one filling the viewport.
    let best: { phase: { name: string; color: string }; area: number } | null = null;
    for (const phase of phases.value) {
      const rect = phaseElements.get(phase.key)?.getBoundingClientRect();
      if (!rect) continue;
      const visible = Math.min(rect.bottom, rootRect.bottom) - Math.max(rect.top, rootRect.top);
      if (visible > 0 && (!best || visible > best.area)) {
        best = { phase: { name: phase.name, color: phase.color }, area: visible };
      }
    }
    activePhase.value = best?.phase ?? null;
    if (best) {
      badgeStyle.value = {
        top: `${rootRect.top + 10}px`,
        right: `${window.innerWidth - rootRect.right + 20}px`,
      };
    }
  }

  function scheduleActivePhaseUpdate() {
    if (frame) return;
    frame = requestAnimationFrame(updateActivePhase);
  }

  onMounted(() => {
    scrollRoot = rootEl.value?.closest(".admin-content") as HTMLElement | null;
    (scrollRoot ?? window).addEventListener("scroll", scheduleActivePhaseUpdate, { passive: true });
    window.addEventListener("resize", scheduleActivePhaseUpdate);
    scheduleActivePhaseUpdate();
  });

  onUnmounted(() => {
    stopClock();
    (scrollRoot ?? window).removeEventListener("scroll", scheduleActivePhaseUpdate);
    window.removeEventListener("resize", scheduleActivePhaseUpdate);
    if (frame) cancelAnimationFrame(frame);
  });

  return {
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
  };
}