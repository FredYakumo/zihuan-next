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
.mode-selection {
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

.mode-cards {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.mode-card {
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

  &.recommended {
    border-color: var(--admin-accent);
    background: var(--admin-accent-soft);
  }
}

.mode-icon {
  font-size: 28px;
  flex-shrink: 0;
}

.mode-content {
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
}
</style>
