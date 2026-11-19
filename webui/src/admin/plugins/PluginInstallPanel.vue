<template>
  <div class="plugin-install-panel">
      <div v-if="installStep === 'picker'" class="component-picker">
        <p class="component-picker-title">选择组件类型</p>
        <div class="component-grid">
          <button v-for="item in componentOptions" :key="item.value" type="button" :class="['component-card', { active: form.component_type === item.value }]" @click="selectComponent(item.value)">
            <img class="component-card-icon" :src="item.icon" :alt="item.label" />
            <strong class="component-card-name">{{ item.label }}</strong>
            <span class="component-card-desc">{{ item.desc }}</span>
          </button>
        </div>
      </div>
      <div v-else-if="installStep === 'config'" class="install-config">
      <div class="selected-component-bar">
        <img class="selected-component-icon" :src="selectedComponent.icon" :alt="selectedComponent.label" />
        <div class="selected-component-info">
          <strong>{{ selectedComponent.label }}</strong>
          <span>{{ selectedComponent.desc }}</span>
        </div>
      </div>
      <t-form label-align="top">
        <div class="form-grid">
          <t-form-item label="插件名称" required><t-input v-model="form.name" /></t-form-item>
          <t-form-item v-if="!isEmbeddedComponent(form.component_type)" label="安装方式" required class="install-method-item">
            <div class="install-method">
              <span v-if="environmentLoading" class="install-method-status">正在检测本机安装能力...</span>
              <template v-else>
                <label :class="{ unavailable: !dockerSupported }" :title="dockerUnsupportedReason"><input v-model="form.install_method" type="radio" value="docker" :disabled="!dockerSupported" /> 本机 Docker <small v-if="!dockerSupported">（不可用）</small></label>
                <label v-if="supportsNativeInstall(form.component_type)" :class="{ unavailable: !nativeSupported }" :title="nativeSupported ? '' : nativeUnsupportedReason"><input v-model="form.install_method" type="radio" value="native" :disabled="!nativeSupported" /> 本机 native 安装 <small v-if="!nativeSupported">（不可用）</small></label>
                <label v-else-if="!isDockerOnlyComponent(form.component_type)" :class="{ unavailable: !binarySupported }" :title="binaryUnsupportedReason"><input v-model="form.install_method" type="radio" value="binary" :disabled="!binarySupported" /> 本机 native 程序 <small v-if="!binarySupported">（不可用）</small></label>
                <label><input v-model="form.install_method" type="radio" value="command_docker" /> Docker 安装命令</label>
                <label v-if="supportsNativeInstall(form.component_type)"><input v-model="form.install_method" type="radio" value="command_native" /> native 安装命令</label>
                <label v-if="!isDockerOnlyComponent(form.component_type) && !supportsNativeInstall(form.component_type)"><input v-model="form.install_method" type="radio" value="command_binary" /> native 程序安装命令</label>
              </template>
            </div>
          </t-form-item>
        </div>
        <template v-if="!environmentLoading">
        <div v-if="!isEmbeddedComponent(form.component_type)" class="global-config">
          <label><input v-model="config.expose_public_access" type="checkbox" /> 暴露公网访问</label>
          <label v-if="config.expose_public_access"><input v-model="config.use_target_machine_address" type="checkbox" /> 使用目标机器地址</label>
          <SetupField v-if="config.expose_public_access && config.use_target_machine_address"><input v-model="config.target_machine_address" placeholder="例如 203.0.113.10" /></SetupField>
        </div>
        <div v-if="form.component_type === 'mysql'" class="form-grid">
          <SetupDeploymentFields v-model="config.relational.deployment" />
          <SetupField label="主机"><input v-model="config.relational.host" /></SetupField>
          <SetupField label="端口"><input v-model.number="config.relational.deployment.port" type="number" min="1" /></SetupField>
          <SetupField label="用户名"><input v-model="config.relational.username" /></SetupField>
          <SetupField label="密码"><SetupCredentialInput v-model="config.relational.password" /></SetupField>
          <SetupField label="数据库"><input v-model="config.relational.database" /></SetupField>
          <SetupField label="最大连接数"><input v-model.number="config.relational.max_connections" type="number" min="1" /></SetupField>
          <SetupField label="获取连接超时（秒）"><input v-model.number="config.relational.acquire_timeout_secs" type="number" min="1" /></SetupField>
        </div>
        <div v-else-if="form.component_type === 'redis'" class="form-grid">
          <SetupDeploymentFields v-model="config.redis.deployment" />
          <SetupField label="Redis URL"><input v-model="config.redis.url" /></SetupField>
          <SetupField label="用户名"><input v-model="config.redis.username" /></SetupField>
          <SetupField label="密码"><SetupCredentialInput v-model="config.redis.password" /></SetupField>
        </div>
        <div v-else-if="form.component_type === 'rustfs'" class="form-grid">
          <SetupDeploymentFields v-model="config.rustfs.deployment" />
          <SetupField label="Endpoint"><input v-model="config.rustfs.endpoint" /></SetupField>
          <SetupField label="Bucket"><input v-model="config.rustfs.bucket" /></SetupField>
          <SetupField label="Region"><input v-model="config.rustfs.region" /></SetupField>
          <SetupField label="Access Key"><SetupCredentialInput v-model="config.rustfs.access_key" input-type="text" /></SetupField>
          <SetupField label="Secret Key"><SetupCredentialInput v-model="config.rustfs.secret_key" /></SetupField>
          <label><input v-model="config.rustfs.path_style" type="checkbox" /> 使用 path-style</label>
        </div>
        <div v-else-if="form.component_type === 'weaviate' || form.component_type === 'elasticsearch'" class="form-grid">
          <SetupDeploymentFields v-model="config.search.deployment" />
          <SetupField label="Base URL"><input v-model="config.search.base_url" /></SetupField>
          <SetupField v-if="form.component_type === 'elasticsearch'" label="用户名"><input v-model="config.search.username" disabled /></SetupField>
          <SetupField :label="form.component_type === 'elasticsearch' ? '密码' : 'API Key'"><SetupCredentialInput v-if="form.component_type === 'elasticsearch'" v-model="config.search.password" /><SetupCredentialInput v-else v-model="config.search.api_key" /></SetupField>
          <SetupField label="向量维度"><input v-model.number="config.search.vector_dimensions" type="number" min="1" /></SetupField>
        </div>
        <t-alert
          v-if="form.component_type === 'wechat_pad'"
          theme="info"
          message="安装 WeChatPadPro本体、配套MySQL、Redis和WebHook Docker容器"
        />
        <t-alert
          v-if="form.component_type === 'napcat'"
          theme="info"
          message="安装 NapCat QQ Bot adapter"
        />
        <t-alert v-if="error" theme="error" :message="error" />
        </template>
      </t-form>
      </div>
      <InstallationProgress
        v-else
        :task-id="taskId"
        :logs="progress"
        :error="error || null"
        :title="installDone ? '安装完成' : '正在安装插件...'"
        :subtitle="installDone ? '插件已就绪，点击确认查看连接信息' : '请稍候，系统正在执行安装和连接配置'"
        back-label="返回配置"
        @retry="install"
        @back="backFromProgress"
      />

  </div>
</template>

<script lang="ts">
import { defineComponent, type PropType } from "vue";
import InstallationProgress from "../components/setup/InstallationProgress.vue";
import SetupCredentialInput from "../components/setup/SetupCredentialInput.vue";
import SetupDeploymentFields from "../components/setup/SetupDeploymentFields.vue";
import SetupField from "../components/setup/SetupField.vue";
import type { UsePluginsPageReturn } from "./Plugins";

export default defineComponent({
  name: "PluginInstallPanel",
  components: {
    InstallationProgress,
    SetupCredentialInput,
    SetupDeploymentFields,
    SetupField,
  },
  props: {
    page: { type: Object as PropType<UsePluginsPageReturn>, required: true },
  },
  setup(props) {
    return { ...props.page };
  },
});
</script>

<style scoped lang="scss">
@use "./plugins" as *;
</style>
