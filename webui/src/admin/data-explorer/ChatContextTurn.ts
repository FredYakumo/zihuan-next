import { computed, onMounted, onUnmounted, ref } from "vue";

import type { ServiceChatContextMedia } from "../../api/client";
import type { TokenUsage } from "../../api/types";
import type { ContextEntryItem, ContextTurnView } from "./useChatContext";

type Segment = { type: "text"; text: string } | { type: "image"; media: ServiceChatContextMedia };

/** Stage name the backend uses for the per-turn assembled system prompt. */
export const SYSTEM_PROMPT_STAGE = "System Prompt";

/** The preprompt run and the stage that reports its result. */
export const PREPROMPT_STAGE = "Preprompt";
export const PREPROMPT_RESULT_STAGE = "Preprompt 输出";

/** Stage the QQ Chat runtime emits with the rate-limit decision of the turn. */
export const RATE_LIMIT_STAGE = "限流";

/**
 * One stage of the turn, wrapping the individual steps it consists of.
 *
 * Stages follow `documents/design/role/qq_chat_role.md`, so the flow reads in the same order
 * as the documented pipeline.
 */
export interface ContextPhase {
  key: string;
  name: string;
  entries: ContextEntryItem[];
}

/**
 * Which stage each reported step belongs to. Stages the runtime reports under their own name
 * still form their own container, so this only needs the steps that belong together.
 */
const PHASE_OF_STAGE: Record<string, string> = {
  [RATE_LIMIT_STAGE]: "触发与限流",
  [PREPROMPT_STAGE]: "BeforeBrain · Preprompt",
  [PREPROMPT_RESULT_STAGE]: "BeforeBrain · Preprompt",
  "Preprompt 阶段": "BeforeBrain · Preprompt",
  "意图识别": "意图识别",
  [SYSTEM_PROMPT_STAGE]: "构建 Brain 输入",
  "情绪与风格提示": "构建 Brain 输入",
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
 * backwards.
 */
export function groupEntriesByPhase(entries: ContextEntryItem[]): ContextPhase[] {
  const phases: ContextPhase[] = [];
  for (const entry of entries) {
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
  text: string;
  /** One-line summary of what produced the text (model, duration, token counts). */
  meta?: string;
  /** The model's thinking for this stage, when the stage recorded it. */
  reasoning?: string;
  /** History messages the stage sent along with `text`, oldest first. */
  history?: StageHistoryMessage[];
}

export interface StageHistoryMessage {
  role: string;
  chars: number;
  preview: string;
}

function detailRecord(detail: unknown): Record<string, unknown> | undefined {
  return detail && typeof detail === "object" ? (detail as Record<string, unknown>) : undefined;
}

function detailText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

/**
 * Readable view of a stage's payload.
 *
 * The prompt-building stages carry one long text each: the assembled system prefix, the
 * preprompt result, and the current-turn input handed to the brain. Showing them as text is the
 * point of the flow view; everything else stays JSON.
 */
export function stageText(stage: string | undefined, detail: unknown): StageText | undefined {
  if (!stage) return undefined;
  if (typeof detail === "string") {
    return stage === SYSTEM_PROMPT_STAGE ? { text: detail } : undefined;
  }
  const record = detailRecord(detail);
  if (!record) return undefined;

  if (stage === SYSTEM_PROMPT_STAGE) {
    const prompt = detailText(record.prompt);
    return prompt ? { text: prompt } : undefined;
  }

  // The preprompt request; its result is reported by the following step in the same stage.
  if (stage === PREPROMPT_STAGE) {
    const lines: string[] = [];
    if (typeof record.model === "string") lines.push(`模型：${record.model}`);
    if (Array.isArray(record.tools_available)) {
      const tools = record.tools_available.filter((item): item is string => typeof item === "string");
      lines.push(`可用工具：${tools.length ? tools.join("、") : "无"}`);
    }
    return lines.length ? { text: lines.join("\n") } : undefined;
  }

  if (stage === PREPROMPT_RESULT_STAGE) {
    const text = detailText(record.context) ?? "(preprompt 未返回上下文)";
    const meta: string[] = [];
    if (typeof record.duration_ms === "number") meta.push(`耗时 ${record.duration_ms} ms`);
    if (typeof record.stop_reason === "string") meta.push(record.stop_reason);
    if (typeof record.tool_calls === "number") meta.push(`工具调用 ${record.tool_calls} 次`);
    return { text, meta: meta.join(" · "), reasoning: detailText(record.reasoning) };
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

  /** Readable text for each stage entry, keyed by entry id. */
  const stageViews = computed(() => {
    const views = new Map<string, StageText | undefined>();
    for (const entry of turn().entries) {
      if (entry.kind !== "stage") continue;
      views.set(entry.id, stageText(entry.stage, entry.detail));
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