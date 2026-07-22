<template>
  <div class="choice-row">
    <label><input v-model="source" type="radio" value="install" /> 安装新组件</label>
    <label><input v-model="source" type="radio" value="existing" /> 使用现有配置</label>
  </div>
</template>

<script setup lang="ts">
import type { DetailedSetupSource } from "../../api/client";

const source = defineModel<DetailedSetupSource>({ required: true });
</script>
