<template>
  <div class="detailed-config-step">
    <h2>详细配置引导</h2>

    <div class="install-method">
      <span>安装方式</span>
      <span v-if="environmentLoading" class="install-method-status">正在检测本机安装能力...</span>
      <template v-else>
        <label :class="{ unavailable: !dockerSupported }" :title="dockerUnsupportedReason">
          <input v-model="selectedInstallMethod" type="radio" value="docker" :disabled="!dockerSupported" />
          Docker <small v-if="!dockerSupported">（本机不支持）</small>
        </label>
        <label :class="{ unavailable: !environment.binary_install_available }" :title="environment.binary_install_reason ?? ''">
          <input v-model="selectedInstallMethod" type="radio" value="binary" :disabled="!environment.binary_install_available" />
          二进制 <small v-if="!environment.binary_install_available">（本机不支持）</small>
        </label>
        <span v-if="isWindows" class="windows-environment">
          WSL：{{ environment.wsl_available ? "可用" : "不可用" }}；
          WSL Docker：{{ environment.wsl_docker_available ? "可用" : "不可用" }}
        </span>
      </template>
    </div>

    <template v-if="selectedInstallMethod">
      <div class="component-scroll">
      <section class="component-card">
        <ComponentHeader v-model:enabled="model.relational.enabled" title="关系型数据库" />
        <template v-if="model.relational.enabled">
          <div class="choice-row"><label><input v-model="model.relational.type" type="radio" value="mysql" /> MySQL</label><label><input v-model="model.relational.type" type="radio" value="sqlite" /> SQLite3</label></div>
          <SourceChoice v-model="model.relational.source" />
          <template v-if="model.relational.type === 'mysql'">
            <DeploymentFields v-if="model.relational.source === 'install'" v-model="model.relational.deployment" />
            <div class="form-grid"><Field label="主机"><input v-model="model.relational.host" /></Field><Field label="端口"><input v-model.number="model.relational.deployment.port" type="number" min="1" /></Field><Field label="用户名"><input v-model="model.relational.username" /></Field><Field label="密码"><CredentialInput v-model="model.relational.password" /></Field><Field label="数据库"><input v-model="model.relational.database" /></Field><Field label="最大连接数"><input v-model.number="model.relational.max_connections" type="number" min="1" /></Field><Field label="获取连接超时（秒）"><input v-model.number="model.relational.acquire_timeout_secs" type="number" min="1" /></Field></div>
          </template>
          <Field v-else label="数据库文件路径"><input v-model="model.relational.sqlite_path" /></Field>
        </template>
      </section>

      <section class="component-card">
        <ComponentHeader v-model:enabled="model.rustfs.enabled" title="RustFS" />
        <template v-if="model.rustfs.enabled"><SourceChoice v-model="model.rustfs.source" /><DeploymentFields v-if="model.rustfs.source === 'install'" v-model="model.rustfs.deployment" /><div class="form-grid"><Field label="Endpoint"><input v-model="model.rustfs.endpoint" /></Field><Field label="Bucket"><input v-model="model.rustfs.bucket" /></Field><Field label="Region"><input v-model="model.rustfs.region" /></Field><Field label="Access Key"><CredentialInput v-model="model.rustfs.access_key" input-type="text" /></Field><Field label="Secret Key"><CredentialInput v-model="model.rustfs.secret_key" /></Field></div><label class="choice-row"><input v-model="model.rustfs.path_style" type="checkbox" /> 使用 path-style</label></template>
      </section>

      <section class="component-card">
        <ComponentHeader v-model:enabled="model.search.enabled" title="检索数据库" />
        <template v-if="model.search.enabled"><div class="choice-row"><label><input :checked="model.search.type === 'weaviate'" type="radio" @change="setSearchType('weaviate')" /> Weaviate</label><label><input :checked="model.search.type === 'elasticsearch'" type="radio" @change="setSearchType('elasticsearch')" /> Elasticsearch</label></div><SourceChoice v-model="model.search.source" /><DeploymentFields v-if="model.search.source === 'install'" v-model="model.search.deployment" /><div class="form-grid"><Field label="Base URL"><input v-model="model.search.base_url" /></Field><Field label="用户名"><input v-model="model.search.username" /></Field><Field label="密码"><CredentialInput v-model="model.search.password" /></Field><Field label="API Key"><CredentialInput v-model="model.search.api_key" /></Field><Field label="向量维度"><input v-model.number="model.search.vector_dimensions" type="number" min="1" /></Field></div></template>
      </section>

      <section class="component-card">
        <ComponentHeader v-model:enabled="model.redis.enabled" title="Redis" hint="可选" />
        <template v-if="model.redis.enabled"><SourceChoice v-model="model.redis.source" /><DeploymentFields v-if="model.redis.source === 'install'" v-model="model.redis.deployment" /><div class="form-grid"><Field label="Redis URL"><input v-model="model.redis.url" /></Field><Field label="用户名"><input v-model="model.redis.username" /></Field><Field label="密码"><CredentialInput v-model="model.redis.password" /></Field></div></template>
      </section>
      </div>

      <div class="step-actions"><button class="btn ghost" @click="$emit('back')"><ArrowLeftIcon /> 返回</button><button class="btn primary" @click="$emit('next')">开始配置 <ArrowRightIcon /></button></div>
    </template>

    <div v-if="!selectedInstallMethod" class="step-actions step-actions--selection">
      <button class="btn ghost" @click="$emit('back')"><ArrowLeftIcon /> 返回</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { ArrowLeftIcon, ArrowRightIcon } from "tdesign-icons-vue-next";

import type { DetailedSetupConfig, DetailedSetupInstallMethod } from "../../api/client";
import { setup as setupApi, type EnvironmentInfo } from "../../api/client";
import ComponentHeader from "./SetupComponentHeader.vue";
import CredentialInput from "./SetupCredentialInput.vue";
import DeploymentFields from "./SetupDeploymentFields.vue";
import Field from "./SetupField.vue";
import SourceChoice from "./SetupSourceChoice.vue";

const model = defineModel<DetailedSetupConfig>({ required: true });
defineEmits<{ (event: "next"): void; (event: "back"): void }>();

const environmentLoading = ref(true);
const selectedInstallMethod = ref<DetailedSetupInstallMethod | null>(null);
const environment = ref<EnvironmentInfo>({
  os: "",
  os_detail: "",
  docker_available: false,
  docker_compose_available: false,
  binary_install_available: false,
  binary_install_reason: null,
  wsl_available: null,
  wsl_docker_available: null,
  cuda_version: null,
  compiler_version: null,
  proxy: null,
  services: [],
});
const dockerSupported = computed(() => environment.value.docker_compose_available);
const dockerUnsupportedReason = computed(() => "Docker Compose 不可用，请安装并启动 Docker Desktop 或 Docker Compose");
const isWindows = computed(() => environment.value.os.toLowerCase() === "windows");

onMounted(async () => {
  try {
    environment.value = await setupApi.getEnvironment();
  } catch (error) {
    console.error("Failed to detect setup environment", error);
  } finally {
    environmentLoading.value = false;
  }
});

watch(selectedInstallMethod, (installMethod) => {
  if (installMethod) {
    model.value.install_method = installMethod;
  }
});

function setSearchType(type: "weaviate" | "elasticsearch") {
  model.value.search.type = type;
  if (type === "elasticsearch") {
    model.value.search.deployment = { ...model.value.search.deployment, image: "docker.elastic.co/elasticsearch/elasticsearch:8.17.0", port: 9200, container_name: "zihuan-elasticsearch" };
    model.value.search.base_url = "http://127.0.0.1:9200";
    model.value.search.username = "elastic";
  } else {
    model.value.search.deployment = { ...model.value.search.deployment, image: "cr.weaviate.io/semitechnologies/weaviate:1.30.5", port: 8080, container_name: "zihuan-weaviate" };
    model.value.search.base_url = "http://127.0.0.1:8080";
  }
}

</script>

<style scoped lang="scss">
@use "../styles/detailed-config-step" as *;
</style>
