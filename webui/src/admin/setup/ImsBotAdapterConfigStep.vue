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
        <LogoQqIcon class="platform-icon" />
        <strong>QQ</strong>
        <span class="platform-tag">NapCat</span>
      </button>

      <button
        class="platform-card disabled"
        disabled
        title="即将支持"
      >
        <ChatIcon class="platform-icon" />
        <strong>微信</strong>
        <span class="platform-tag coming-soon">即将支持</span>
      </button>

      <button
        class="platform-card disabled"
        disabled
        title="即将支持"
      >
        <SendIcon class="platform-icon" />
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
      <button class="btn ghost" @click="$emit('back')"><ArrowLeftIcon /> 返回</button>
      <button
        class="btn primary"
        :disabled="!canProceed"
        @click="$emit('next')"
      >
        开始安装 <ArrowRightIcon />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ArrowLeftIcon, ArrowRightIcon, ChatIcon, LogoQqIcon, SendIcon } from "tdesign-icons-vue-next";

import type { ImsBotAdapterSetupConfig } from "../../api/client";
import { useImsBotAdapterConfigStep } from "../composables/useImsBotAdapterConfigStep";

const model = defineModel<ImsBotAdapterSetupConfig>({ required: true });

defineEmits<{ (e: "next"): void; (e: "back"): void }>();

const { canProceed, platformHint } = useImsBotAdapterConfigStep(model);
</script>

<style scoped lang="scss">
@use "../styles/ims-bot-adapter-config-step" as *;
</style>
