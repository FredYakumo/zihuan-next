<template>
  <div ref="container" class="credential-input" :class="{ 'has-visibility-control': inputType === 'password' }">
    <input v-model="value" :type="isVisible ? 'text' : inputType" @blur="isVisible = false" />
    <button
      v-if="inputType === 'password'"
      class="credential-visibility-button"
      type="button"
      :aria-label="isVisible ? '隐藏凭据' : '显示凭据'"
      :title="isVisible ? '隐藏凭据' : '显示凭据'"
      @mousedown.prevent
      @click="isVisible = !isVisible"
    >
      <BrowseOffIcon v-if="isVisible" />
      <BrowseIcon v-else />
    </button>
    <button
      class="credential-help-button"
      type="button"
      aria-label="生成高强度凭据建议"
      title="生成高强度凭据建议"
      :aria-expanded="isOpen"
      @click="isOpen = !isOpen"
    >
      <ErrorCircleIcon />
    </button>
    <div v-if="isOpen" class="credential-help-popover" role="dialog" aria-label="高强度凭据建议">
      <button type="button" @click="suggestCredential">建议可用高强度密码/key</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { BrowseIcon, BrowseOffIcon, ErrorCircleIcon } from "tdesign-icons-vue-next";

withDefaults(defineProps<{ inputType?: "password" | "text" }>(), { inputType: "password" });

const value = defineModel<string | null>({ required: true });
const container = ref<HTMLElement | null>(null);
const isOpen = ref(false);
const isVisible = ref(false);

const credentialCharacters = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

function suggestCredential() {
  const randomBytes = new Uint32Array(32);
  crypto.getRandomValues(randomBytes);
  value.value = Array.from(randomBytes, (byte) => credentialCharacters[byte % credentialCharacters.length]).join("");
  isOpen.value = false;
}

function closeWhenClickingOutside(event: MouseEvent) {
  if (!container.value?.contains(event.target as Node)) {
    isOpen.value = false;
  }
}

onMounted(() => document.addEventListener("click", closeWhenClickingOutside));
onBeforeUnmount(() => document.removeEventListener("click", closeWhenClickingOutside));
</script>

<style scoped lang="scss">
.credential-input {
  position: relative;
}

.credential-input input {
  padding-right: 38px;
}

.credential-input.has-visibility-control input {
  padding-right: 66px;
}

.credential-help-button,
.credential-visibility-button {
  position: absolute;
  top: 50%;
  right: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  color: var(--admin-muted);
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
  transform: translateY(-50%);
}

.credential-help-button {
  right: 8px;
}

.credential-visibility-button {
  right: 36px;
}

.credential-help-button:hover,
.credential-help-button:focus-visible,
.credential-visibility-button:hover,
.credential-visibility-button:focus-visible {
  color: var(--admin-primary);
  background: color-mix(in srgb, var(--admin-primary) 10%, transparent);
  outline: none;
}

.credential-help-popover {
  position: absolute;
  z-index: 2;
  top: calc(100% + 6px);
  right: 0;
  padding: 6px;
  background: var(--admin-bg-panel);
  border: 1px solid var(--admin-border);
  border-radius: 6px;
  box-shadow: 0 8px 20px rgb(0 0 0 / 12%);
}

.credential-help-popover button {
  padding: 7px 10px;
  color: var(--admin-text);
  white-space: nowrap;
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
}

.credential-help-popover button:hover,
.credential-help-popover button:focus-visible {
  background: color-mix(in srgb, var(--admin-primary) 10%, transparent);
  outline: none;
}
</style>
