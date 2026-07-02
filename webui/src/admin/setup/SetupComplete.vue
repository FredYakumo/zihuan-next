<template>
  <div class="setup-complete">
    <div class="complete-icon">✅</div>
    <h2>设置完成！</h2>
    <p class="subtitle">Zihuan Next 已准备好使用</p>

    <button class="btn primary" @click="$emit('finish')">
      进入主界面 →
    </button>
  </div>
</template>

<script setup lang="ts">
defineEmits<{ (e: "finish"): void }>();
</script>

<style scoped lang="scss">
@use "../styles/setup-complete" as *;
</style>
