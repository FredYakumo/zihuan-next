<template>
  <div class="detailed-install-command-result">
    <div>
      <h2>安装命令已生成</h2>
      <p class="subtitle">在目标 Linux 机器上执行安装命令，然后将连接配置导入系统配置的 connections 字段。</p>
    </div>

    <section class="command-output">
      <div class="command-output__header">
        <h3>安装命令</h3>
        <button class="btn ghost" @click="copyText(result.install_command, 'command')">
          <CopyIcon /> {{ copied === 'command' ? '已复制' : '复制' }}
        </button>
      </div>
      <textarea readonly :value="result.install_command" aria-label="安装命令" />
    </section>

    <section class="command-output">
      <div class="command-output__header">
        <h3>连接配置 JSON</h3>
        <button class="btn ghost" @click="copyText(connectionsJson, 'connections')">
          <CopyIcon /> {{ copied === 'connections' ? '已复制' : '复制' }}
        </button>
      </div>
      <textarea readonly :value="connectionsJson" aria-label="连接配置 JSON" />
    </section>

    <p v-if="copyError" class="config-error">{{ copyError }}</p>
    <div class="step-actions">
      <button class="btn ghost" @click="$emit('back')"><ArrowLeftIcon /> 返回配置</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ArrowLeftIcon, CopyIcon } from "tdesign-icons-vue-next";

import type { DetailedInstallCommandResult } from "../../api/client";
import { useDetailedInstallCommandResult } from "./DetailedInstallCommandResult";

const props = defineProps<{ result: DetailedInstallCommandResult }>();
defineEmits<{ (event: "back"): void }>();

const { copied, copyError, connectionsJson, copyText } = useDetailedInstallCommandResult(() => props.result);
</script>

<style scoped lang="scss">
@use "./detailed-install-command-result" as *;
</style>
