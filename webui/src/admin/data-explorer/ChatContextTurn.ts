import { computed, ref } from "vue";

import type { ServiceChatContextMedia } from "../../api/client";
import type { TokenUsage } from "../../api/types";
import type { ContextTurnView } from "./useChatContext";

type Segment = { type: "text"; text: string } | { type: "image"; media: ServiceChatContextMedia };

/** Stage name the backend uses for the per-turn assembled system prompt. */
export const SYSTEM_PROMPT_STAGE = "System Prompt";

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
