<template>
  <div class="napcat-config-step">
    <h2>配置 NapCat</h2>
    <p class="subtitle">设置 QQ Bot 连接信息</p>

    <div class="form-grid">
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

    <div class="actions">
      <button class="btn ghost" @click="$emit('back')">← 返回</button>
      <button
        class="btn primary"
        :disabled="!model.ws_url.trim()"
        @click="$emit('next')"
      >
        开始安装 →
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { NapCatSetupConfig } from "../../api/client";

const model = defineModel<NapCatSetupConfig>({ required: true });

defineEmits<{ (e: "next"): void; (e: "back"): void }>();
</script>

<style scoped lang="scss">
.napcat-config-step {
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

.actions {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
</style>
