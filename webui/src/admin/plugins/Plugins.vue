<template>
  <section class="page plugins-page">
    <AdminPageHeader title="插件" />
    <t-card title="已安装插件" bordered header-bordered>
      <template #actions><t-button theme="primary" @click="openInstall">安装插件</t-button></template>
      <t-table :data="plugins" :columns="columns" row-key="id" :loading="loading" :pagination="false" size="small">
        <template #status="{ row }"><t-tag :theme="statusTheme(row.status)" variant="light" :title="statusTitle(row)">{{ statusLabel(row.status) }}</t-tag></template>
        <template #connection_ids="{ row }"><span>{{ row.connection_ids?.length ?? 0 }} 个连接</span></template>
        <template #actions="{ row }"><t-space size="small"><t-button v-if="installCommand(row)" variant="text" @click="copyPluginValue(installCommand(row)!, '安装命令')"><template #icon><CopyIcon /></template>复制安装命令</t-button><t-button v-if="connectionConfigJson(row)" variant="text" @click="copyPluginValue(connectionConfigJson(row)!, '连接 JSON')"><template #icon><CopyIcon /></template>复制连接 JSON</t-button><t-button v-if="row.status === 'installed'" variant="text" @click="toggle(row)">停用</t-button><t-button v-else-if="row.status === 'disabled'" variant="text" @click="toggle(row)">启用</t-button><t-popconfirm content="确认卸载插件并删除关联连接吗？" @confirm="removePlugin(row.id)"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm></t-space></template>
      </t-table>
    </t-card>
    <t-dialog
      :visible="installVisible"
      width="720px"
      :close-on-overlay-click="false"
      @close="requestCloseInstall"
    >
      <template #header>
        <div class="dialog-header">
          <t-button v-if="installStep === 'config'" variant="text" size="small" class="dialog-back" @click="backFromConfig"><ChevronLeftIcon /></t-button>
          <span class="dialog-title">{{ installStep === "progress" ? "安装进度" : "安装插件" }}</span>
        </div>
      </template>
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
          <t-form-item v-if="form.component_type !== 'sqlite'" label="安装方式" required class="install-method-item">
            <div class="install-method">
              <span v-if="environmentLoading" class="install-method-status">正在检测本机安装能力...</span>
              <template v-else>
                <label :class="{ unavailable: !dockerSupported }" :title="dockerUnsupportedReason"><input v-model="form.install_method" type="radio" value="docker" :disabled="!dockerSupported" /> 本机 Docker <small v-if="!dockerSupported">（不可用）</small></label>
                <label :class="{ unavailable: !binarySupported }" :title="binaryUnsupportedReason"><input v-model="form.install_method" type="radio" value="binary" :disabled="!binarySupported" /> 本机 native 程序 <small v-if="!binarySupported">（不可用）</small></label>
                <label><input v-model="form.install_method" type="radio" value="command_docker" /> Docker 安装命令</label>
                <label><input v-model="form.install_method" type="radio" value="command_binary" /> native 程序安装命令</label>
              </template>
            </div>
          </t-form-item>
        </div>
        <template v-if="!environmentLoading">
        <div v-if="form.component_type !== 'sqlite'" class="global-config">
          <label><input v-model="config.expose_public_access" type="checkbox" /> 暴露公网访问</label>
          <label v-if="config.expose_public_access"><input v-model="config.use_target_machine_address" type="checkbox" /> 使用目标机器地址</label>
          <SetupField v-if="config.expose_public_access && config.use_target_machine_address" label="目标机器地址（可选）"><input v-model="config.target_machine_address" placeholder="例如 203.0.113.10" /></SetupField>
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
        <t-alert v-if="error" theme="error" :message="error" />
        </template>
      </t-form>
      </div>
      <InstallationProgress
        v-else
        :task-id="taskId"
        :logs="progress"
        :error="error || null"
        title="正在安装插件..."
        subtitle="请稍候，系统正在执行安装和连接配置"
        back-label="返回配置"
        @retry="install"
        @back="backFromProgress"
      />
      <template v-if="installStep !== 'progress'" #footer>
        <div class="dialog-footer">
          <t-button variant="outline" @click="requestCloseInstall">取消</t-button>
          <t-button theme="primary" :loading="saving" :disabled="installStep !== 'config' || (form.component_type !== 'sqlite' && environmentLoading)" @click="install">开始安装</t-button>
        </div>
      </template>
    </t-dialog>
    <t-dialog
      v-model:visible="exitConfirmVisible"
      header="退出安装界面？"
      confirm-btn="退出"
      cancel-btn="继续等待"
      :close-on-overlay-click="false"
      @confirm="confirmExitInstallation"
    >
      安装任务会继续在后台运行。退出后可以在插件列表中查看当前状态，请勿重复提交安装。
    </t-dialog>
    <InstallationSuccessDialog
      :visible="successVisible"
      :install-command="successInstallCommand"
      :connection-config="successConnectionConfig"
      @confirm="confirmInstallationSuccess"
    />
    <t-dialog v-model:visible="uninstallCommandVisible" header="卸载命令" :confirm-btn="null" cancel-btn="关闭">
      <div class="plugin-command-dialog">
        <t-button variant="text" shape="square" @click="copyPluginValue(uninstallCommand, '卸载命令')"><template #icon><CopyIcon /></template></t-button>
        <t-textarea :value="uninstallCommand" readonly autosize />
      </div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { ChevronLeftIcon, CopyIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import InstallationSuccessDialog from "../components/setup/InstallationSuccessDialog.vue";
import InstallationProgress from "../components/setup/InstallationProgress.vue";
import SetupCredentialInput from "../components/setup/SetupCredentialInput.vue";
import SetupDeploymentFields from "../components/setup/SetupDeploymentFields.vue";
import SetupField from "../components/setup/SetupField.vue";
import { usePluginsPage } from "./Plugins";

const {
  plugins,
  loading,
  saving,
  installVisible,
  installStep,
  successVisible,
  successInstallCommand,
  successConnectionConfig,
  uninstallCommandVisible,
  uninstallCommand,
  exitConfirmVisible,
  error,
  progress,
  taskId,
  componentOptions,
  form,
  selectedComponent,
  environmentLoading,
  dockerSupported,
  binarySupported,
  dockerUnsupportedReason,
  binaryUnsupportedReason,
  config,
  columns,
  statusLabel,
  statusTheme,
  statusTitle,
  selectComponent,
  openInstall,
  requestCloseInstall,
  backFromConfig,
  confirmExitInstallation,
  backFromProgress,
  confirmInstallationSuccess,
  install,
  toggle,
  installCommand,
  connectionConfigJson,
  copyPluginValue,
  removePlugin,
} = usePluginsPage();
</script>

<style scoped lang="scss">
@use "./plugins" as *;
</style>
