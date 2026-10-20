import { computed, ref } from "vue";

import type { ServiceChatContextMedia } from "../../api/client";
import type { TokenUsage } from "../../api/types";
import type { ContextTurnView } from "./useChatContext";

type Segment = { type: "text"; text: string } | { type: "image"; media: ServiceChatContextMedia };

/** Stage name the backend uses for the per-turn assembled system prompt. */
export const SYSTEM_PROMPT_STAGE = "System Prompt";

/** Stage the QQ Chat runtime emits with the rate-limit decision of the turn. */
export const RATE_LIMIT_STAGE = "限流";

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

/** System prompt text of a stage entry, or undefined when the entry is another stage. */
export function systemPromptText(stage: string, detail: unknown): string | undefined {
  if (stage !== SYSTEM_PROMPT_STAGE) return undefined;
  if (typeof detail === "string") return detail;
  if (detail && typeof detail === "object" && "prompt" in detail) {
    const prompt = (detail as { prompt?: unknown }).prompt;
    return typeof prompt === "string" ? prompt : undefined;
  }
  return undefined;
}

export function useChatContextTurn(turn: () => ContextTurnView) {
  const previewUrl = ref("");
  const failedMedia = ref(new Set<string>());
  // Thinking starts collapsed, matching the Chat page.
  const thinkingOpen = ref(false);
  const mediaIdPattern = /media_id=([^\s\]]+)/g;

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
    failedMedia.value.add(media.media_id);
  }

  function openPreview(media: ServiceChatContextMedia) {
    if (media.url) previewUrl.value = media.url;
  }

  function closePreview() {
    previewUrl.value = "";
  }

  return {
    previewUrl,
    thinkingOpen,
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