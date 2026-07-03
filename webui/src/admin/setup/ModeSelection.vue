<template>
  <div class="mode-selection">
    <h2>欢迎使用 Zihuan Next</h2>

    <div class="mode-cards">
      <button class="mode-card recommended" @click="$emit('select', 'role')">
        <div class="mode-content">
          <strong>按使用角色配置（推荐）</strong>
          <span>根据您的使用场景快速完成配置</span>
        </div>
      </button>

      <button class="mode-card" @click="$emit('select', 'detailed')">
        <div class="mode-content">
          <strong>详细配置引导（高级）</strong>
          <span>适合有经验的用户</span>
        </div>
      </button>

      <button class="mode-card" @click="$emit('select', 'skip')">
        <div class="mode-content">
          <strong>跳过安装向导</strong>
        </div>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
defineEmits<{ (e: "select", mode: "role" | "detailed" | "skip"): void }>();
</script>

<style scoped lang="scss">
@use "../styles/mode-selection" as *;
</style>
