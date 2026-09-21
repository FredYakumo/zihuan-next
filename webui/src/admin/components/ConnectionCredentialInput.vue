<template>
  <div class="connection-credential-input">
    <input
      v-model="value"
      :class="inputClass"
      :type="isVisible ? 'text' : 'password'"
      :placeholder="placeholder"
      @blur="isVisible = false"
    />
    <button
      class="connection-credential-visibility-button"
      type="button"
      :aria-label="isVisible ? '隐藏凭据' : '临时显示凭据'"
      :title="isVisible ? '隐藏凭据' : '临时显示凭据'"
      @mousedown.prevent
      @click="isVisible = !isVisible"
    >
      <BrowseOffIcon v-if="isVisible" />
      <BrowseIcon v-else />
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { BrowseIcon, BrowseOffIcon } from "tdesign-icons-vue-next";

withDefaults(defineProps<{ placeholder?: string; inputClass?: string }>(), {
  placeholder: "",
  inputClass: "",
});

const value = defineModel<string>({ required: true });
const isVisible = ref(false);
</script>

<style scoped lang="scss">
@use "./connection-credential-input" as *;
</style>
