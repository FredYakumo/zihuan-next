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
   * container so the turn (with its round header) starts the viewport, then flash it briefly.
   */
  async function scrollToTurn(turnId: string) {
    const index = allTurns.value.findIndex((item) => item.turn.turnId === turnId);
    if (index < 0) return;
    // Reveal one extra step above the target so it keeps context and the anchor sits clear of
    // the scroll-load threshold at the top instead of triggering another history load.
    const needed = Math.min(allTurns.value.length - index + HISTORY_STEP, allTurns.value.length);
    if (resolvedVisibleCount.value < needed) visibleCount.value = needed;
    window.clearTimeout(flashTimer);
    flashTurnId.value = turnId;
    await nextTick();
    // The round header renders before its turns, so the first match is the anchor to scroll to.
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
        behavior: "smooth",
      });
    } else {
      target.scrollIntoView({ block: "start", behavior: "smooth" });
    }
    flashTimer = setTimeout(() => {
      flashTurnId.value = "";
    }, 1600);
  }

  onMounted(() => {
    scrollRoot = rootEl.value?.closest(".admin-content") as HTMLElement | null;
    (scrollRoot ?? window).addEventListener("scroll", onScroll, { passive: true });
    void fillUntilScrollable();
  });

  onUnmounted(() => {
    (scrollRoot ?? window).removeEventListener("scroll", onScroll);
    window.clearTimeout(flashTimer);
    window.clearTimeout(glideTimer);
  });

  return {
    rootEl,
    allTurns,
    visibleItems,
    flashTurnId,
    scrollToTurn,
  };
}
