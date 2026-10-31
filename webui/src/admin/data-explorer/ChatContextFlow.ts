import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import type { ContextBackend, ContextTurnView } from "./useChatContext";

export function formatRate(value?: number): string {
  return value === undefined ? "—" : `${value.toFixed(1)} Tokens/s`;
}

export function formatPercent(value?: number): string {
  return value === undefined ? "—" : `${(value * 100).toFixed(1)}%`;
}

export interface ChatContextFlowProps {
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

interface FlowTurn {
  turn: ContextTurnView;
  source: "archived" | "live";
}

const HISTORY_STEP = 5;
const SCROLL_LOAD_THRESHOLD = 80;

export function useChatContextFlow(props: ChatContextFlowProps) {
  const rootEl = ref<HTMLElement | null>(null);

  /** Archive and live window in display order: oldest first, archive before live. */
  const allTurns = computed<FlowTurn[]>(() => [
    ...props.archivedTurns.map((turn) => ({ turn, source: "archived" as const })),
    ...props.turns.map((turn) => ({ turn, source: "live" as const })),
  ]);


  const visibleCount = ref(0);

  /** The newest round is the last turn plus the steer follow-ups continuing it. */
  function latestRoundCount(turns: FlowTurn[]): number {
    if (!turns.length) return 0;
    let count = 1;
    for (let index = turns.length - 1; index > 0 && turns[index].turn.continuesPrevious; index -= 1) {
      count += 1;
    }
    return count;
  }

  const resolvedVisibleCount = computed(() => Math.max(visibleCount.value, latestRoundCount(allTurns.value)));
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

  let scrollRoot: HTMLElement | null = null;

  function loadMoreHistory() {
    if (!hasMoreHistory.value) return;
    // Older rounds are prepended, so keep the viewport on the turns being read: the same visual
    // position is a larger scrollTop once the new content is in. The flow opts out of the
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

  // Selecting another user restarts the flow at its newest round.
  watch(
    () => props.selectedSenderId,
    () => {
      visibleCount.value = 0;
    },
  );

  /** If the rendered tail is too short to scroll, reveal more until the view overflows or ends. */
  async function fillUntilScrollable() {
    await nextTick();
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
    if (!hasMoreHistory.value) return;
    const top = scrollRoot ? scrollRoot.scrollTop : window.scrollY;
    if (top <= SCROLL_LOAD_THRESHOLD) loadMoreHistory();
  }

  onMounted(() => {
    scrollRoot = rootEl.value?.closest(".admin-content") as HTMLElement | null;
    (scrollRoot ?? window).addEventListener("scroll", onScroll, { passive: true });
    void fillUntilScrollable();
  });

  onUnmounted(() => {
    (scrollRoot ?? window).removeEventListener("scroll", onScroll);
  });

  return {
    rootEl,
    allTurns,
    visibleItems,
  };
}
