<template>
  <div class="role-selection">
    <h2>选择使用角色</h2>
    <p class="subtitle">我们将根据您的选择为您配置最合适的方案</p>

    <div class="role-cards">
      <button class="role-card" @click="$emit('select', 'chat_assistant')">
        <div class="role-content">
          <strong>对话聊天助手</strong>
          <span>类似ChatGPT、DeepSeek的网页对话助手</span>
        </div>
      </button>

      <button class="role-card" @click="$emit('select', 'code_dev_assistant')">
        <div class="role-content">
          <strong>项目开发助手</strong>
          <span>专注于编程的开发辅助AI助手</span>
        </div>
      </button>

      <button class="role-card" @click="$emit('select', 'qq_chat_bot')">
        <div class="role-content">
          <strong>QQChat机器人</strong>
          <span>QQ智能机器人</span>
        </div>
      </button>

      <button class="role-card" @click="$emit('select', 'ai_butler')">
        <div class="role-content">
          <strong>后台 AI 助理管家</strong>
          <span>全能 AI 管家，类似 OpenClaw</span>
        </div>
      </button>
    </div>

    <button class="btn ghost back-btn" @click="$emit('back')"><ArrowLeftIcon /> 返回</button>
  </div>
</template>

<script setup lang="ts">
import { ArrowLeftIcon } from "tdesign-icons-vue-next";

defineEmits<{
  (e: "select", role: "chat_assistant" | "code_dev_assistant" | "qq_chat_bot" | "ai_butler"): void;
  (e: "back"): void;
}>();
</script>

<style scoped lang="scss">
@use "../styles/role-selection" as *;
</style>
