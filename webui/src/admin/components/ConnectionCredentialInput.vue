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
.connection-credential-input {
  position: relative;
  width: 100%;
}

.connection-credential-input input {
  width: 100%;
  padding-right: 42px;
}

.connection-credential-input .connection-card-inline-input {
  min-height: 32px;
  padding: 6px 42px 6px 10px;
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  background: color-mix(in srgb, var(--admin-bg-panel) 92%, white 8%);
  color: var(--admin-ink);
  font: inherit;
}

.connection-credential-visibility-button {
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

.connection-credential-visibility-button:hover,
.connection-credential-visibility-button:focus-visible {
  color: var(--admin-primary);
  background: color-mix(in srgb, var(--admin-primary) 10%, transparent);
  outline: none;
}
</style>
