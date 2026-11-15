<template>
  <t-dialog
    :visible="state.visible"
    :header="header"
    :footer="false"
    :close-btn="state.phase !== 'restarting'"
    :close-on-overlay-click="false"
    width="640px"
    @update:visible="(value: boolean) => !value && emit('close')"
  >
    <template v-if="state.mode === 'save'">
      <p class="service-restart-dialog__notice">该项目关联如下 Role Service，需要重启 Role Service 以生效：</p>
      <ul class="service-restart-dialog__list">
        <li v-for="service in state.services" :key="service.id">
          <span class="service-restart-dialog__name">{{ service.name }}</span>
          <span class="service-restart-dialog__id">{{ service.id }}</span>
        </li>
      </ul>
      <t-alert theme="warning" message="仅保存配置但是不重启服务，配置可能不会立马生效并可能会带来配置不一致性的问题。" />
      <t-checkbox
        v-if="state.phase === 'confirm'"
        :checked="confirmChecked"
        label="确认重启Role Service"
        class="service-restart-dialog__checkbox"
        @change="(checked: boolean) => (confirmChecked = checked)"
      />
      <div
        v-if="state.phase === 'restarting' || state.phase === 'done' || state.phase === 'failed'"
        class="service-restart-dialog__logs"
      >
        <p v-if="state.phase === 'restarting'" class="service-restart-dialog__progress">{{ state.progressText }}</p>
        <p v-if="state.phase === 'done'" class="service-restart-dialog__done">保存并重启服务成功</p>
        <p v-if="state.phase === 'failed'" class="service-restart-dialog__failed">重启失败：{{ state.errorMessage }}</p>
        <div ref="logContainer" class="service-restart-dialog__log-panel">
          <div
            v-for="(line, index) in state.logLines"
            :key="index"
            class="service-restart-dialog__log-line"
            :class="{ 'service-restart-dialog__log-line--error': isErrorLevel(line.level) }"
          >[{{ line.timestamp }}] {{ line.level }} {{ line.message }}</div>
        </div>
      </div>
      <div class="service-restart-dialog__footer">
        <template v-if="state.phase === 'confirm'">
          <t-button theme="primary" :disabled="!confirmChecked" @click="emit('restart')">保存并重启服务</t-button>
          <t-button variant="outline" @click="emit('save-only')">仅保存</t-button>
          <t-button variant="outline" @click="emit('close')">取消</t-button>
        </template>
        <t-button v-else-if="state.phase === 'restarting'" theme="primary" disabled loading>正在重启服务...</t-button>
        <t-button v-else theme="primary" @click="emit('close')">关闭</t-button>
      </div>
    </template>
    <template v-else>
      <p class="service-restart-dialog__notice">该项目关联如下运行中的 Role Service，已自动停止以避免配置不一致：</p>
      <ul class="service-restart-dialog__list">
        <li v-for="service in state.services" :key="service.id">
          <span class="service-restart-dialog__name">{{ service.name }}</span>
          <span class="service-restart-dialog__id">{{ service.id }}</span>
        </li>
      </ul>
      <t-alert theme="warning" message="这些服务引用的配置已删除，需要重新配置后再启动。" />
      <div class="service-restart-dialog__footer">
        <t-button theme="primary" @click="emit('close')">关闭</t-button>
      </div>
    </template>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";

import type { ServiceRestartDialogState } from "./useServiceRestart";

const props = defineProps<{
  state: ServiceRestartDialogState;
}>();

const emit = defineEmits<{
  /** 取消 / 关闭: dismiss the dialog only; the page keeps its editor open. */
  (event: "close"): void;
  /** 仅保存: config saved without restart; the page closes its editor. */
  (event: "save-only"): void;
  (event: "restart"): void;
}>();

const confirmChecked = ref(false);
const logContainer = ref<HTMLElement | null>(null);

const header = computed(() =>
  props.state.mode === "save" ? "保存并重启 Role Service" : "关联的运行中 Role Service 已停止",
);

function isErrorLevel(level: string): boolean {
  const normalized = level.toUpperCase();
  return normalized === "ERROR" || normalized === "ERROR_DETAIL";
}

// Streamed logs should follow the newest line, like a terminal.
watch(
  () => props.state.logLines.length,
  async () => {
    await nextTick();
    logContainer.value?.scrollTo({ top: logContainer.value.scrollHeight });
  },
);

// Reopening the dialog must start from an unchecked confirmation box.
watch(
  () => props.state.visible,
  (visible) => {
    if (visible) {
      confirmChecked.value = false;
    }
  },
);
</script>

<style scoped lang="scss">
.service-restart-dialog {
  &__notice {
    margin: 0 0 8px;
  }

  &__list {
    margin: 0 0 12px;
    padding-left: 20px;

    li {
      line-height: 1.8;
    }
  }

  &__name {
    font-weight: 600;
  }

  &__id {
    margin-left: 8px;
    color: var(--td-text-color-secondary, #888);
    font-family: monospace;
    font-size: 12px;
  }

  &__checkbox {
    margin: 12px 0;
  }

  &__logs {
    margin-top: 8px;
  }

  &__progress {
    margin: 0 0 6px;
    color: var(--td-brand-color, #0052d9);
  }

  &__done {
    margin: 0 0 6px;
    color: var(--td-success-color, #2ba471);
    font-weight: 600;
  }

  &__failed {
    margin: 0 0 6px;
    color: var(--td-error-color, #d54941);
    font-weight: 600;
  }

  &__log-panel {
    max-height: 260px;
    overflow-y: auto;
    padding: 8px;
    background: var(--td-bg-color-component, #f3f3f3);
    border-radius: 6px;
    font-family: monospace;
    font-size: 12px;
    line-height: 1.6;
  }

  &__log-line {
    white-space: pre-wrap;
    word-break: break-all;

    &--error {
      color: var(--td-error-color, #d54941);
    }
  }

  &__footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
}
</style>
