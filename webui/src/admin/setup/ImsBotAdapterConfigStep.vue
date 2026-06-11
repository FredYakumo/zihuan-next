<template>
  <div class="ims-bot-adapter-config-step">
    <h2>配置 IMS Bot Adapter</h2>
    <p class="subtitle">选择消息平台并设置连接信息</p>

    <div class="platform-cards">
      <button
        class="platform-card"
        :class="{ selected: model.platform === 'qq_napcat' }"
        @click="model.platform = 'qq_napcat'"
      >
        <div class="platform-icon">🐧</div>
        <strong>QQ</strong>
        <span class="platform-tag">NapCat</span>
      </button>

      <button
        class="platform-card disabled"
        disabled
        title="即将支持"
      >
        <div class="platform-icon">💬</div>
        <strong>微信</strong>
        <span class="platform-tag coming-soon">即将支持</span>
      </button>

      <button
        class="platform-card disabled"
        disabled
        title="即将支持"
      >
        <div class="platform-icon">✈️</div>
        <strong>Telegram</strong>
        <span class="platform-tag coming-soon">即将支持</span>
      </button>
    </div>

    <div v-if="model.platform === 'qq_napcat'" class="form-grid">
      <div class="field">
        <label>WebSocket 地址</label>
        <input
          v-model="model.ws_url"
          type="text"
          placeholder="ws://127.0.0.1:3001"
        />
      </div>

      <div class="field">
        <label>QQ 号（可选）</label>
        <input
          v-model="model.qq_id"
          type="text"
          placeholder="机器人的 QQ 号"
        />
      </div>

      <div class="field">
        <label>Token（可选）</label>
        <input
          v-model="model.token"
          type="password"
          placeholder="NapCat 连接 Token"
        />
      </div>
    </div>

    <div v-else class="form-grid">
      <div class="field disabled-hint">
        <p>{{ platformHint }}</p>
      </div>
    </div>

    <div class="actions">
      <button class="btn ghost" @click="$emit('back')">← 返回</button>
      <button
        class="btn primary"
        :disabled="!canProceed"
        @click="$emit('next')"
      >
        开始安装 →
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { ImsBotAdapterSetupConfig } from "../../api/client";

const model = defineModel<ImsBotAdapterSetupConfig>({ required: true });

defineEmits<{ (e: "next"): void; (e: "back"): void }>();

const canProceed = computed(() => {
  if (model.value.platform === "qq_napcat") {
    return model.value.ws_url.trim().length > 0;
  }
  return false;
});

const platformHint = computed(() => {
  switch (model.value.platform) {
    case "wechat":
      return "微信适配器即将支持，敬请期待。";
    case "telegram":
      return "Telegram 适配器即将支持，敬请期待。";
    default:
      return "";
  }
});
</script>

<style scoped lang="scss">
.ims-bot-adapter-config-step {
  text-align: center;

  h2 {
    margin: 0 0 8px;
    font-size: 22px;
    color: var(--admin-ink);
  }

  .subtitle {
    margin: 0 0 24px;
    color: var(--admin-muted);
    font-size: 15px;
  }
}

.platform-cards {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
  margin-bottom: 24px;
}

.platform-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 16px 12px;
  border: 2px solid var(--admin-border);
  border-radius: 10px;
  background: var(--admin-bg);
  cursor: pointer;
  transition: border-color 0.2s, background 0.2s;

  .platform-icon {
    font-size: 32px;
  }

  strong {
    font-size: 15px;
    color: var(--admin-ink);
  }

  .platform-tag {
    font-size: 12px;
    color: var(--admin-muted);
    background: var(--admin-border);
    padding: 2px 8px;
    border-radius: 10px;

    &.coming-soon {
      color: var(--admin-accent);
      background: var(--admin-accent-light, rgba(99, 102, 241, 0.1));
    }
  }

  &.selected {
    border-color: var(--admin-accent);
    background: var(--admin-accent-light, rgba(99, 102, 241, 0.08));
  }

  &.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 16px;
  text-align: left;
  margin-bottom: 24px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;

  label {
    font-size: 13px;
    font-weight: 600;
    color: var(--admin-ink);
  }

  input {
    padding: 10px 12px;
    border: 1px solid var(--admin-border);
    border-radius: 6px;
    background: var(--admin-bg);
    color: var(--admin-ink);
    font-size: 14px;

    &:focus {
      outline: none;
      border-color: var(--admin-accent);
    }
  }
}

.disabled-hint {
  text-align: center;
  padding: 24px;

  p {
    color: var(--admin-muted);
    font-size: 14px;
    margin: 0;
  }
}

.actions {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
</style>
