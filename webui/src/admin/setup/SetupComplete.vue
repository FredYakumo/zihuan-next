<template>
  <div class="setup-complete">
    <CheckCircleIcon class="complete-icon" />
    <h2>设置完成！</h2>
    <p class="subtitle">Zihuan Next 已准备好使用</p>

    <button class="btn primary" @click="$emit('finish')">
      进入主界面 <ArrowRightIcon />
    </button>
  </div>
</template>

<script setup lang="ts">
import { ArrowRightIcon, CheckCircleIcon } from "tdesign-icons-vue-next";

defineEmits<{ (e: "finish"): void }>();
</script>

<style scoped lang="scss">
@use "../styles/setup-complete" as *;
</style>
