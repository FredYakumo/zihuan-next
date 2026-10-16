<template>
  <section class="page data-explorer-page">
    <AdminPageHeader title="数据检索" />
    <nav class="explorer-entry-grid" aria-label="数据检索入口">
      <RouterLink class="explorer-entry" to="/data-explorer/models">
        <span class="entry-icon" aria-hidden="true"><ChartBarIcon /></span>
        <span class="entry-title">模型统计</span>
        <ChevronRightIcon class="entry-arrow" aria-hidden="true" />
      </RouterLink>
      <RouterLink class="explorer-entry" to="/data-explorer/services">
        <span class="entry-icon" aria-hidden="true"><ServerIcon /></span>
        <span class="entry-title">Service 数据</span>
        <ChevronRightIcon class="entry-arrow" aria-hidden="true" />
      </RouterLink>
    </nav>
  </section>
</template>

<script setup lang="ts">
import { ChartBarIcon, ChevronRightIcon, ServerIcon } from "tdesign-icons-vue-next";
import { RouterLink } from "vue-router";
</script>

<style scoped lang="scss">
.explorer-entry-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  max-width: 652px;
}

.explorer-entry {
  display: flex;
  align-items: center;
  gap: 12px;
  box-sizing: border-box;
  min-height: 80px;
  padding: 16px;
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  background: var(--admin-bg-panel);
  color: var(--admin-ink);
  text-decoration: none;
  transition: border-color 160ms ease, background-color 160ms ease;

  &:hover {
    border-color: var(--admin-accent);
    background: var(--admin-bg-panel-strong);
  }

  &:focus-visible {
    outline: 2px solid var(--admin-accent);
    outline-offset: 4px;
  }

  &:hover .entry-arrow,
  &:focus-visible .entry-arrow {
    color: var(--admin-accent);
  }
}

.entry-icon {
  display: grid;
  place-items: center;
  flex: 0 0 36px;
  height: 36px;
  border-radius: 10px;
  background: var(--admin-accent-soft);
  color: var(--admin-accent);
  font-size: 20px;
}

.entry-title { min-width: 0; font-size: 15px; font-weight: 600; line-height: 1.4; }
.entry-arrow { flex-shrink: 0; margin-left: auto; color: var(--admin-subtle); font-size: 18px; }

@media (max-width: 480px) {
  .explorer-entry-grid { grid-template-columns: 1fr; }
}

@media (prefers-reduced-motion: reduce) {
  .explorer-entry { transition: none; }
}
</style>
