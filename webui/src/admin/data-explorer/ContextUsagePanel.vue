<template>
  <aside
    ref="panelEl"
    class="context-usage-panel"
    :class="[`context-usage-panel--${tone}`, { 'context-usage-panel--collapsed': collapsed, 'context-usage-panel--dragging': dragging }]"
    :style="panelStyle"
    @pointerdown="startDrag"
    @pointermove="moveDrag"
    @pointerup="endDrag"
    @pointercancel="endDrag"
  >
    <button
      v-if="collapsed"
      type="button"
      class="context-usage-orb"
      :style="{ '--usage': `${usedPercent}%` }"
      :title="`当前上下文 ${usedLabel} / ${limitLabel}（${percentLabel}），点击展开，按住拖动`"
      @click="toggleCollapsed"
    >
      <span class="context-usage-orb-label">{{ percentLabel }}</span>
    </button>
    <template v-else>
      <div class="context-usage-head">
        <span class="context-usage-title">当前上下文</span>
        <span class="context-usage-badge">{{ percentLabel }}</span>
        <button type="button" class="context-usage-collapse" title="收起悬浮窗" @click="collapsed = true">▴</button>
      </div>
      <div class="context-usage-bar"><span :style="{ width: `${usedPercent}%` }"></span></div>
      <div class="context-usage-tokens">
        <strong>{{ usedLabel }}</strong>
        <span class="context-usage-tokens-limit">/ {{ limitLabel }}</span>
      </div>
      <dl class="context-usage-meta">
        <div><dt>会话开始</dt><dd>{{ formatTimeOrDash(startedAt) }}</dd></div>
        <div><dt>最后消息</dt><dd>{{ formatTimeOrDash(lastMessageAt) }}</dd></div>
      </dl>
      <t-button
        class="context-usage-clear"
        theme="danger"
        variant="outline"
        size="small"
        block
        :loading="clearing"
        :disabled="busy"
        :title="busy ? '当前有回复正在生成，结束后才能清除上下文' : ''"
        @click="$emit('clear')"
      >{{ busy ? "回复生成中…" : "清除上下文" }}</t-button>
    </template>
  </aside>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";

import { formatTime } from "../model";

const props = defineProps<{
  usedTokens: number;
  /** Context window of the service's primary model; absent when it cannot be resolved. */
  limitTokens?: number;
  /** Oldest turn still held in the live context, i.e. where this context segment started. */
  startedAt?: string;
  lastMessageAt?: string;
  clearing: boolean;
  /** A turn is still generating; clearing then would race the runtime writing it back. */
  busy?: boolean;
}>();

defineEmits<{ clear: [] }>();

const collapsed = ref(false);

const usedPercent = computed(() => {
  if (!props.limitTokens || props.limitTokens < 1) return 0;
  return Math.min((props.usedTokens / props.limitTokens) * 100, 100);
});
const percentLabel = computed(() => props.limitTokens ? `${usedPercent.value.toFixed(1)}%` : "—");
const usedLabel = computed(() => props.usedTokens.toLocaleString());
const limitLabel = computed(() => props.limitTokens ? `${props.limitTokens.toLocaleString()} tokens` : "上限未知");
const tone = computed(() => {
  if (!props.limitTokens) return "normal";
  if (usedPercent.value >= 90) return "danger";
  if (usedPercent.value >= 75) return "warning";
  return "normal";
});

// The floating window keeps its default top-right spot until the user drags it; from then on it
// lives at explicit viewport coordinates, so collapse/expand keeps the chosen place.
const panelEl = ref<HTMLElement | null>(null);
const draggedPos = ref<{ x: number; y: number } | null>(null);
const dragging = ref(false);
let dragPointerId: number | null = null;
let dragMoved = false;
const dragStart = { x: 0, y: 0 };
const dragOrigin = { x: 0, y: 0 };
/** Minimum on-screen margin so a dragged panel can never leave a grabbable sliver. */
const DRAG_MARGIN = 8;
/** Pointer travel before a gesture counts as a drag instead of a click on the orb. */
const DRAG_THRESHOLD = 3;

const panelStyle = computed(() =>
  draggedPos.value
    ? { left: `${draggedPos.value.x}px`, top: `${draggedPos.value.y}px`, right: "auto" }
    : undefined,
);

function toggleCollapsed() {
  // A drag that started on the orb ends with a synthetic click; it must not expand the panel.
  if (dragMoved) {
    dragMoved = false;
    return;
  }
  collapsed.value = false;
}

function startDrag(event: PointerEvent) {
  if (dragPointerId !== null || event.button !== 0) return;
  const el = panelEl.value;
  if (!el) return;
  // Only the floating form drags; the in-flow form on narrow screens stays where it is.
  if (getComputedStyle(el).position !== "fixed") return;
  const target = event.target as Element | null;
  // Interactive controls inside the panel keep their normal click behavior — except the orb,
  // which drags while held and only expands on a click that was not a drag.
  const onOrb = target?.closest(".context-usage-orb") != null;
  if (!onOrb && target?.closest("button, a, input, textarea, select")) return;
  const pressed = (event.target as HTMLElement) ?? el;
  dragPointerId = event.pointerId;
  dragMoved = false;
  dragStart.x = event.clientX;
  dragStart.y = event.clientY;
  const rect = el.getBoundingClientRect();
  dragOrigin.x = rect.left;
  dragOrigin.y = rect.top;
  dragging.value = true;
  // Capture on the pressed element so its own click still fires on release; the pointer events
  // bubble up to this component's handlers either way.
  pressed.setPointerCapture(event.pointerId);
}

function moveDrag(event: PointerEvent) {
  if (dragPointerId !== event.pointerId) return;
  const el = panelEl.value;
  if (!el) return;
  if (!dragMoved && Math.hypot(event.clientX - dragStart.x, event.clientY - dragStart.y) < DRAG_THRESHOLD) {
    return;
  }
  dragMoved = true;
  const rect = el.getBoundingClientRect();
  const x = Math.min(
    Math.max(dragOrigin.x + event.clientX - dragStart.x, DRAG_MARGIN),
    window.innerWidth - rect.width - DRAG_MARGIN,
  );
  const y = Math.min(
    Math.max(dragOrigin.y + event.clientY - dragStart.y, DRAG_MARGIN),
    window.innerHeight - rect.height - DRAG_MARGIN,
  );
  draggedPos.value = { x, y };
}

function endDrag(event: PointerEvent) {
  if (dragPointerId !== event.pointerId) return;
  dragPointerId = null;
  dragging.value = false;
}

function formatTimeOrDash(value?: string): string {
  return value ? formatTime(value) : "—";
}
</script>

<style scoped lang="scss">
@use "./ContextUsagePanel" as *;
</style>
