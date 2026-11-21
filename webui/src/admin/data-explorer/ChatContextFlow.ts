import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import { explorer, type ServiceChatStep } from "../../api/client";
import type { QqSubAgentEntry } from "../../api/types";
import type { ContextBackend, ContextTurnView, ContextEntryItem } from "./useChatContext";

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? value as Record<string, unknown> : {};
}

/** Adapt durable steps to the existing ordered, categorized flow renderer. */
function stepEntries(step: ServiceChatStep): ContextEntryItem[] {
  const input = record(step.input);
  const output = record(step.output);
  const at = typeof input.at === "string" ? input.at : step.started_at;
  if (input.kind === "tool_started") {
    return [{ kind: "tool", id: `${step.turn_id}:tool:${String(input.call_id)}`, callId: String(input.call_id),
      name: String(input.tool_name ?? step.name), arguments: JSON.stringify(input.arguments ?? {}, null, 2),
      result: typeof output.result === "string" ? output.result : undefined,
      durationMs: step.duration_ms ?? undefined, running: step.status === "running" }];
  }
  if (input.kind === "sub_agent") {
    const events = output.kind === "sub_agent" ? [input, output] : [input];
    return events.map((event, index) => ({ kind: "sub-agent", id: `${step.step_id}:${index}`, at,
      path: Array.isArray(event.path) ? event.path as string[] : [], event: event.event as QqSubAgentEntry }));
  }
  const detail = { ...record(input.detail), ...record(output.detail) };
  if (!Array.isArray(detail.messages) && Array.isArray(detail.request)) {
    detail.messages = detail.request.map((raw) => {
      const message = record(raw);
      const parts = Array.isArray(message.parts) ? message.parts : [];
      return { ...message, content: parts.map((part) => {
        const value = record(part);
        return typeof value.text === "string" ? value.text : JSON.stringify(part);
      }).join("\n") };
    });
  }
  if (step.status === "interrupted") { detail.status = "error"; detail.error = "执行中断"; }
  return [{ kind: "stage", id: step.step_id, stageId: typeof input.id === "string" ? input.id : step.step_id,
    at, stage: typeof input.stage === "string" ? input.stage : step.name, detail }];
}

function entryIdentity(entry: ContextEntryItem): string {
  if (entry.kind === "stage") return `stage:${entry.stageId ?? entry.id}`;
  if (entry.kind === "tool") return `tool:${entry.callId}`;
  return `sub-agent:${JSON.stringify(entry.path)}:${JSON.stringify(entry.event)}`;
}

export function formatRate(value?: number): string {
  return value === undefined ? "—" : `${value.toFixed(1)} Tokens/s`;
}

export function formatPercent(value?: number): string {
  return value === undefined ? "—" : `${(value * 100).toFixed(1)}%`;
}

export interface ChatContextFlowProps {
  serviceId: string;
  backend: ContextBackend;
  loadingContext: boolean;
  turns: ContextTurnView[];
  archivedTurns: ContextTurnView[];
  estimatedContextTokens: number;
  inputTokens: number;
  outputTokens: number;
  averageTokensPerSecond?: number;
  averageCacheHitRate?: number;
  selectedSenderId: string;
  deletingTurnId: string;
}

export interface ChatContextFlowEmits {
  "delete-turn": [turn: ContextTurnView];
}

/** What the parent may call on the flow, e.g. to jump to a turn picked in the finder. */
export interface ChatContextFlowExposed {
  scrollToTurn(turnId: string): void;
}

interface FlowTurn {
  turn: ContextTurnView;
  source: "archived" | "live";
}

const HISTORY_STEP = 5;
const SCROLL_LOAD_THRESHOLD = 80;
const GLIDE_SETTLE_MS = 1200;

export function useChatContextFlow(props: ChatContextFlowProps) {
  const rootEl = ref<HTMLElement | null>(null);
  let mounted = false;
  let initialPositionPending = true;
  let initialPositionVersion = 0;

  /** Archive and live window in display order: oldest first, archive before live. */
  const allTurns = computed<FlowTurn[]>(() => [
    ...props.archivedTurns.map((turn) => ({ turn, source: "archived" as const })),
    ...props.turns.map((turn) => ({ turn, source: "live" as const })),
  ]);


  const visibleCount = ref(0);

  /**
   * A turn group is one turn plus the steer follow-ups that continue it: one user question as
   * the reader perceives it. A steer turn is a separate turn in the backend (its own `turn_id`),
   * but the flow keeps the group visually together. See `documents/glossary.md`.
   */
  function latestTurnGroupCount(turns: FlowTurn[]): number {
    if (!turns.length) return 0;
    let count = 1;
    for (let index = turns.length - 1; index > 0 && turns[index].turn.continuesPrevious; index -= 1) {
      count += 1;
    }
    return count;
  }

  const resolvedVisibleCount = computed(() => Math.max(visibleCount.value, latestTurnGroupCount(allTurns.value)));
  const visibleTurns = computed(() => allTurns.value.slice(-resolvedVisibleCount.value));
  const hiddenCount = computed(() => Math.max(allTurns.value.length - resolvedVisibleCount.value, 0));
  const hasMoreHistory = computed(() => resolvedVisibleCount.value < allTurns.value.length);

  const visibleItems = computed(() =>
    visibleTurns.value.map((item, index) => ({
      ...item,
      showArchiveLabel: item.source === "archived" && index === 0,
      showArchiveDivider:
        item.source === "live" && index > 0 && visibleTurns.value[index - 1]?.source === "archived",
      showHeader: index === 0 || !item.turn.continuesPrevious,
    })),
  );

  const stepLoads = new Map<string, Promise<ContextEntryItem[]>>();

  async function loadPersistedTurn(turn: ContextTurnView, retry = false) {
    if (!turn.stepsAvailable) return;
    const sender = props.selectedSenderId;
    const key = `${sender}:${turn.turnId}:${turn.finished}`;
    if (retry) stepLoads.delete(key);
    let pending = stepLoads.get(key);
    if (!pending) {
      pending = (async () => {
        const directory = (await explorer.queryServiceChatSteps(props.serviceId, sender, turn.turnId)).items;
        const results: ContextEntryItem[][] = new Array(directory.length);
        let position = 0;
        await Promise.all(Array.from({ length: Math.min(directory.length, 4) }, async () => {
          while (position < directory.length) {
            const index = position++;
            const step = directory[index];
            const body = (await explorer.queryServiceChatSteps(props.serviceId, sender, turn.turnId, step.step_id)).items[0];
            if (body) results[index] = stepEntries(body);
          }
        }));
        return results.flat().filter(Boolean);
      })();
      stepLoads.set(key, pending);
    }
    turn.entriesLoading = true;
    turn.entriesError = undefined;
    try {
      const saved = await pending;
      if (sender !== props.selectedSenderId) return;
      const merged = new Map(saved.map((entry) => [entryIdentity(entry), entry]));
      if (!turn.finished) turn.entries.forEach((entry) => merged.set(entryIdentity(entry), entry));
      turn.entries = [...merged.values()];
    } catch (error) {
      stepLoads.delete(key);
      turn.entriesError = (error as Error).message;
    } finally { turn.entriesLoading = false; }
  }

  watch(() => visibleTurns.value.map(({ turn }) => turn), (turns) => {
    turns.forEach((turn) => { void loadPersistedTurn(turn); });
  }, { immediate: true });

  let scrollRoot: HTMLElement | null = null;

  function loadMoreHistory() {
    if (!hasMoreHistory.value) return;
    // Older turn groups are prepended, so keep the viewport on the turns being read: the same
    // visual position is a larger scrollTop once the new content is in. The flow opts out of the
    // browser's scroll anchoring to apply this adjustment itself.
    const previousHeight = scrollRoot?.scrollHeight ?? 0;
    const previousTop = scrollRoot?.scrollTop ?? 0;
    visibleCount.value = resolvedVisibleCount.value + HISTORY_STEP;
    if (scrollRoot) {
      void nextTick(() => {
        if (scrollRoot) scrollRoot.scrollTop = scrollRoot.scrollHeight - previousHeight + previousTop;
      });
    }
  }

  // Selecting another user restarts the flow at its newest turn group.
  watch(
    () => props.selectedSenderId,
    () => {
      visibleCount.value = 0;
      stepLoads.clear();
      initialPositionPending = true;
      initialPositionVersion += 1;
    },
  );

  watch(() => props.archivedTurns.length, (count, previous) => {
    if (previous > 0 && count > previous) {
      visibleCount.value = allTurns.value.length;
    }
  });

  /** If the rendered tail is too short to scroll, reveal more until the view overflows or ends. */
  async function fillUntilScrollable() {
    await nextTick();
    if (props.loadingContext || initialPositionPending) return;
    if (!hasMoreHistory.value) return;
    const scrollable = scrollRoot
      ? scrollRoot.scrollHeight > scrollRoot.clientHeight + 1
      : document.documentElement.scrollHeight > window.innerHeight + 1;
    if (!scrollable) loadMoreHistory();
  }

  watch(resolvedVisibleCount, () => {
    void fillUntilScrollable();
  });

  function onScroll() {
    if (initialPositionPending || props.loadingContext) return;
    if (glidingToAnchor) return;
    if (!hasMoreHistory.value) return;
    const top = scrollRoot ? scrollRoot.scrollTop : window.scrollY;
    if (top <= SCROLL_LOAD_THRESHOLD) loadMoreHistory();
  }

  const flashTurnId = ref("");
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  /** True while a jump is gliding: scroll events must not trigger a mid-flight history load. */
  let glidingToAnchor = false;
  let glideTimer: ReturnType<typeof setTimeout> | undefined;

  /**
   * Mute scroll-load while a jump glides: loading history prepends turns above the flow, which
   * would both shift the anchor and cancel the browser's smooth scroll with an instant write.
   */
  function muteScrollLoadWhileGliding() {
    glidingToAnchor = true;
    window.clearTimeout(glideTimer);
    glideTimer = setTimeout(() => {
      glidingToAnchor = false;
    }, GLIDE_SETTLE_MS);
  }

  /**
   * Bring a turn picked in the finder into view: reveal enough history, glide the page
   * container so the turn (with its turn-group header) starts the viewport, then flash it
   * briefly.
   */
  async function scrollToTurn(turnId: string, initial = false) {
    const index = allTurns.value.findIndex((item) => item.turn.turnId === turnId);
    if (index < 0) return;
    // Reveal one extra step above the target so it keeps context and the anchor sits clear of
    // the scroll-load threshold at the top instead of triggering another history load.
    const needed = Math.min(allTurns.value.length - index + (initial ? 0 : HISTORY_STEP), allTurns.value.length);
    if (resolvedVisibleCount.value < needed) visibleCount.value = needed;
    window.clearTimeout(flashTimer);
    if (!initial) flashTurnId.value = turnId;
    await nextTick();
    // The turn-group header renders before its turns, so the first match is the anchor.
    const target = Array.from(rootEl.value?.querySelectorAll<HTMLElement>("[data-turn-id]") ?? []).find(
      (el) => el.dataset.turnId === turnId,
    );
    if (!target) return;
    // Glide so the jump reads as a move instead of a teleport; the flash is timed to still be
    // lit on arrival. Always smooth: the machine may report reduced motion, but the jump is
    // exactly the interaction the glide was asked for.
    muteScrollLoadWhileGliding();
    if (scrollRoot) {
      const stickyHead = scrollRoot.querySelector<HTMLElement>(".context-detail-head");
      const stickyOffset = stickyHead ? stickyHead.offsetHeight : 0;
      scrollRoot.scrollTo({
        top:
          target.getBoundingClientRect().top -
          scrollRoot.getBoundingClientRect().top +
          scrollRoot.scrollTop -
          stickyOffset,
        behavior: initial ? "instant" : "smooth",
      });
    } else {
      target.scrollIntoView({ block: "start", behavior: initial ? "instant" : "smooth" });
    }
    flashTimer = setTimeout(() => {
      flashTurnId.value = "";
    }, 1600);
  }

  async function positionLatestTurn() {
    if (!mounted || !initialPositionPending || props.loadingContext) return;
    const version = ++initialPositionVersion;
    const sender = props.selectedSenderId;
    // Steer follow-ups belong to the same conversation block; land at its beginning.
    const group = allTurns.value.slice(-latestTurnGroupCount(allTurns.value));
    if (!group.length) return;
    await Promise.all(group.map(({ turn }) => loadPersistedTurn(turn)));
    await nextTick();
    if (!mounted || version !== initialPositionVersion || sender !== props.selectedSenderId) return;
    await scrollToTurn(group[0].turn.turnId, true);
    initialPositionPending = false;
  }

  watch(() => [props.selectedSenderId, props.loadingContext, allTurns.value.length], () => {
    void positionLatestTurn();
  }, { flush: "post" });

  function cancelInitialPosition() {
    initialPositionPending = false;
    initialPositionVersion += 1;
  }

  onMounted(() => {
    mounted = true;
    scrollRoot = rootEl.value?.closest(".admin-content") as HTMLElement | null;
    (scrollRoot ?? window).addEventListener("scroll", onScroll, { passive: true });
    (scrollRoot ?? window).addEventListener("wheel", cancelInitialPosition, { passive: true });
    (scrollRoot ?? window).addEventListener("touchstart", cancelInitialPosition, { passive: true });
    void positionLatestTurn();
    void fillUntilScrollable();
  });

  onUnmounted(() => {
    mounted = false;
    initialPositionVersion += 1;
    (scrollRoot ?? window).removeEventListener("scroll", onScroll);
    (scrollRoot ?? window).removeEventListener("wheel", cancelInitialPosition);
    (scrollRoot ?? window).removeEventListener("touchstart", cancelInitialPosition);
    window.clearTimeout(flashTimer);
    window.clearTimeout(glideTimer);
  });

  return {
    rootEl,
    allTurns,
    visibleItems,
    flashTurnId,
    scrollToTurn,
    loadPersistedTurn,
  };
}
