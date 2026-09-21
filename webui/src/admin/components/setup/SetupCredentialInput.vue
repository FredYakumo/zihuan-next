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
@use "./setup-credential-input" as *;
</style>
