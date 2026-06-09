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

    <button class="btn ghost back-btn" @click="$emit('back')">← 返回</button>
  </div>
</template>

<script setup lang="ts">
defineEmits<{
  (e: "select", role: "chat_assistant" | "code_dev_assistant" | "qq_chat_bot" | "ai_butler"): void;
  (e: "back"): void;
}>();
</script>

<style scoped lang="scss">
.role-selection {
  text-align: center;

  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    color: var(--admin-ink);
  }

  .subtitle {
    margin: 0 0 28px;
    color: var(--admin-muted);
    font-size: 15px;
  }
}

.role-cards {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-bottom: 24px;
}

.role-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 20px;
  border: 1px solid var(--admin-border);
  border-radius: 10px;
  background: var(--admin-bg);
  cursor: pointer;
  text-align: left;
  transition: border-color 0.2s, box-shadow 0.2s;

  &:hover {
    border-color: var(--admin-accent);
    box-shadow: 0 0 0 3px var(--admin-accent-soft);
  }
}

.role-icon {
  font-size: 28px;
  flex-shrink: 0;
}

.role-content {
  display: flex;
  flex-direction: column;
  gap: 4px;

  strong {
    font-size: 16px;
    color: var(--admin-ink);
  }

  span {
    font-size: 13px;
    color: var(--admin-muted);
  }

  .role-hint {
    font-size: 12px;
    color: var(--admin-accent);
    margin-top: 2px;
  }
}

.back-btn {
  margin-top: 8px;
}
</style>
