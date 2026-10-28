<template>
  <div class="code-viewer">
    <div class="code-viewer-body">
      <div class="code-viewer-gutter" aria-hidden="true">
        <div v-for="line in lineNumbers" :key="line">{{ line }}</div>
      </div>
      <pre class="code-viewer-text">{{ code }}</pre>
    </div>
    <p v-if="notice" class="code-viewer-notice">{{ notice }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

/** Read-only source viewer: line numbers, vertical scroll, sticky gutter on horizontal scroll. */
const props = withDefaults(defineProps<{ code: string; notice?: string }>(), { notice: "" });

const lineNumbers = computed(() => {
  const count = props.code.split("\n").length;
  return Array.from({ length: count }, (_, index) => index + 1);
});
</script>

<style scoped lang="scss">
.code-viewer {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}

/* The viewer owns its scroll box and fills whatever height the parent grants, so the
   vertical scrollbar appears as soon as the content is taller than that box. */
.code-viewer-body {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: auto;
  font: 12px/1.6 ui-monospace, SFMono-Regular, Consolas, monospace;
}

.code-viewer-gutter {
  position: sticky;
  left: 0;
  flex: none;
  padding: 12px 8px;
  border-right: 1px solid var(--td-component-stroke, #e7e7e7);
  background: var(--td-bg-color-container, #fff);
  color: var(--td-text-color-placeholder, rgb(0 0 0 / 40%));
  text-align: right;
  user-select: none;
}

.code-viewer-gutter > div {
  min-height: 1.6em;
}

.code-viewer-text {
  flex: 1;
  margin: 0;
  padding: 12px;
  white-space: pre;
  font: inherit;
}

.code-viewer-notice {
  margin: 6px 0 0;
  color: var(--td-text-color-placeholder, rgb(0 0 0 / 40%));
  font-size: 12px;
}
</style>
