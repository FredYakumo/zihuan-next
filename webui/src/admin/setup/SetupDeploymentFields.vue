<template>
  <div class="form-grid deployment-fields">
    <SetupField label="镜像 / 版本"><input v-model="deployment.image" /></SetupField>
    <SetupField label="端口"><input v-model.number="deployment.port" type="number" min="1" /></SetupField>
    <SetupField label="数据目录"><input v-model="deployment.data_dir" placeholder="./data/容器名" /></SetupField>
    <SetupField label="容器名"><input v-model="deployment.container_name" /></SetupField>
    <SetupField label="重启策略"><select v-model="deployment.restart_policy"><option value="unless-stopped">unless-stopped</option><option value="always">always</option><option value="no">no</option></select></SetupField>
  </div>
</template>

<script setup lang="ts">
import type { DetailedDeploymentConfig } from "../../api/client";
import SetupField from "./SetupField.vue";

const deployment = defineModel<DetailedDeploymentConfig>({ required: true });
</script>
