<template>
  <t-dialog
    v-if="visible"
    :visible="visible"
    :header="false"
    :footer="false"
    width="560px"
    :close-btn="false"
    :close-on-overlay-click="false"
  >
    <section class="installation-success" aria-live="polite">
      <CheckCircleFilledIcon class="installation-success__icon" />
      <h2>{{ title }}</h2>
      <p class="installation-success__description">{{ description }}</p>

      <section v-if="installCommand" class="installation-success__artifact">
        <div class="installation-success__artifact-header">
          <h3>安装命令</h3>
          <t-tooltip content="复制安装命令">
            <t-button variant="text" shape="square" @click="copyText(installCommand, 'command')">
              <CopyIcon />
            </t-button>
          </t-tooltip>
        </div>
        <textarea readonly :value="installCommand" aria-label="安装命令" />
        <span v-if="copied === 'command'" class="installation-success__copied">已复制</span>
      </section>

      <section v-if="connectionConfig !== undefined" class="installation-success__artifact">
        <div class="installation-success__artifact-header">
          <h3>连接配置 JSON</h3>
          <t-tooltip content="复制连接配置">
            <t-button variant="text" shape="square" @click="copyText(connectionConfigJson, 'connections')">
              <CopyIcon />
            </t-button>
          </t-tooltip>
        </div>
        <textarea readonly :value="connectionConfigJson" aria-label="连接配置 JSON" />
        <span v-if="copied === 'connections'" class="installation-success__copied">已复制</span>
      </section>

      <p v-if="copyError" class="installation-success__error">{{ copyError }}</p>
      <t-button theme="primary" size="large" @click="$emit('confirm')">确定</t-button>
    </section>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { CheckCircleFilledIcon, CopyIcon } from "tdesign-icons-vue-next";

const props = withDefaults(defineProps<{
  visible: boolean;
  title?: string;
  description?: string;
  installCommand?: string | null;
  connectionConfig?: unknown;
}>(), {
  title: "安装完成",
  description: "组件已准备就绪，可以开始使用。",
  installCommand: null,
  connectionConfig: undefined,
});

defineEmits<{ (event: "confirm"): void }>();

const copied = ref<"command" | "connections" | null>(null);
const copyError = ref<string | null>(null);
const connectionConfigJson = computed(() => JSON.stringify(props.connectionConfig, null, 2));

async function copyText(value: string, target: "command" | "connections") {
  copyError.value = null;
  try {
    await navigator.clipboard.writeText(value);
    copied.value = target;
    window.setTimeout(() => {
      if (copied.value === target) copied.value = null;
    }, 1600);
  } catch (error) {
    copyError.value = `复制失败：${error instanceof Error ? error.message : String(error)}`;
  }
}
</script>

<style scoped lang="scss">
.installation-success { display: flex; flex-direction: column; align-items: center; gap: 12px; text-align: center; padding: 16px 8px 8px;
  h2, h3, p { margin: 0; }
  > .t-button { min-width: 112px; margin-top: 8px; }
}
.installation-success__icon { width: 84px; height: 84px; color: var(--td-success-color); animation: installation-success-enter 360ms ease-out both; }
.installation-success__description { color: var(--admin-muted); }
.installation-success__artifact { width: 100%; display: grid; gap: 8px; padding: 12px; box-sizing: border-box; border: 1px solid var(--admin-border); border-radius: 8px; text-align: left; }
.installation-success__artifact-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.installation-success__artifact h3 { font-size: 14px; }
.installation-success__artifact textarea { width: 100%; min-height: 120px; box-sizing: border-box; resize: vertical; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; line-height: 1.5; }
.installation-success__copied { color: var(--td-success-color); font-size: 13px; }
.installation-success__error { width: 100%; color: var(--td-error-color); text-align: left; }
@keyframes installation-success-enter { from { opacity: 0; transform: scale(0.55); } to { opacity: 1; transform: scale(1); } }
</style>
