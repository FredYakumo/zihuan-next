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
.setup-complete {
  text-align: center;
  padding: 40px 20px;

  .complete-icon {
    font-size: 56px;
    margin-bottom: 16px;
  }

  h2 {
    margin: 0 0 8px;
    font-size: 24px;
    color: var(--admin-ink);
  }

  .subtitle {
    margin: 0 0 32px;
    color: var(--admin-muted);
    font-size: 15px;
  }
}
</style>
