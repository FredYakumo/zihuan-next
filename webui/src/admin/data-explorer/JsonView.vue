<template>
  <div class="json-view">
    <div v-if="!collapsed" class="json-view-body">
      <pre v-if="!normalized.json || mode === 'raw'" class="json-view-raw">{{ normalized.raw }}</pre>
      <div v-else class="json-view-tree">
        <div
          v-for="row in rows"
          :key="row.id"
          class="json-row"
          :style="{ paddingLeft: `${row.depth * 16}px` }"
        >
          <template v-if="row.close">
            <span class="json-caret-spacer"></span>
            <span class="json-brace">{{ row.kind === "object" ? "}" : "]" }}</span>
            <span v-if="row.comma" class="json-comma">,</span>
          </template>
          <template v-else>
            <button
              v-if="row.expandable"
              type="button"
              class="json-toggle"
              :title="row.expanded ? '折叠' : '展开'"
              @click.stop="toggle(row.id)"
            >
              {{ row.expanded ? "▾" : "▸" }}
            </button>
            <span v-else class="json-caret-spacer"></span>

            <template v-if="row.key !== undefined">
              <span class="json-key" :class="{ 'json-key--index': row.isIndex }">
                {{ row.isIndex ? row.key : `"${row.key}"` }}
              </span>
              <span class="json-colon">: </span>
            </template>

            <template v-if="row.container">
              <span class="json-brace">{{ row.kind === "object" ? "{" : "[" }}</span>
              <template v-if="row.empty">
                <span class="json-brace">{{ row.kind === "object" ? "}" : "]" }}</span>
              </template>
              <template v-else-if="!row.expanded">
                <span class="json-collapsed-hint">… {{ row.entries }} 项</span>
                <span class="json-brace">{{ row.kind === "object" ? "}" : "]" }}</span>
              </template>
              <span v-if="row.comma" class="json-comma">,</span>
            </template>

            <template v-else>
              <span class="json-value" :class="`json-value--${row.kind}`">
                {{ row.expandable && !row.expanded ? row.preview : row.text }}
              </span>
              <span v-if="row.comma" class="json-comma">,</span>
            </template>
          </template>
        </div>
      </div>
    </div>

    <div class="json-view-actions">
      <button v-if="canCopy" type="button" class="json-view-action" @click.stop="copyRaw">
        {{ copied ? "已复制" : "复制原始数据" }}
      </button>
      <button
        v-if="canToggle"
        type="button"
        class="json-view-action"
        :title="toggleTitle"
        @click.stop="toggleMode"
      >
        切换显示
      </button>
      <button type="button" class="json-view-action" @click.stop="collapsed = !collapsed">
        {{ collapsed ? "展开" : "折叠" }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useJsonView } from "./JsonView";

const props = defineProps<{ source: unknown }>();

const {
  normalized,
  rows,
  mode,
  collapsed,
  copied,
  canToggle,
  canCopy,
  toggleTitle,
  toggle,
  toggleMode,
  copyRaw,
} = useJsonView(() => props.source);
</script>

<style scoped lang="scss">
@use "./JsonView" as *;
</style>
