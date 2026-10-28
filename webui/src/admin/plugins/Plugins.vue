<template>
  <section class="page plugins-page">
    <AdminPageHeader title="插件" />
    <t-card title="已安装插件" bordered header-bordered>
      <template #actions><t-space size="small"><t-button variant="outline" @click="openZipInstall">安装 zip 包</t-button><t-button theme="primary" @click="openInstall">安装插件</t-button></t-space></template>
      <t-table :data="plugins" :columns="columns" row-key="id" :loading="loading" :pagination="false" size="small">
        <template #name="{ row }"><div class="plugin-name-cell"><span>{{ row.name }}</span><small v-if="scriptPluginDetail(row)" class="plugin-name-desc" :title="scriptPluginDetail(row)">{{ scriptPluginDetail(row) }}</small></div></template>
        <template #component_type="{ row }"><span :title="row.component_type">{{ componentLabel(row) }}</span></template>
        <template #installation_method="{ row }"><span>{{ installMethodLabel(row.installation_method) }}</span></template>
        <template #status="{ row }"><t-tag :theme="statusTheme(row.status)" variant="light" :title="statusTitle(row)">{{ statusLabel(row.status) }}</t-tag></template>
        <template #connection_ids="{ row }"><span v-if="isScriptPlugin(row)">—</span><span v-else>{{ row.connection_ids?.length ?? 0 }} 个连接</span></template>
        <template #actions="{ row }"><t-space size="small"><template v-if="isScriptPlugin(row)"><t-button variant="text" @click="openDetail(row)">详情</t-button><t-popconfirm content="确认卸载插件并删除其目录吗？" @confirm="removeScriptPlugin(row)"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm></template><template v-else><t-button v-if="installCommand(row)" variant="text" @click="copyPluginValue(installCommand(row)!, '安装命令')"><template #icon><CopyIcon /></template>复制安装命令</t-button><t-button v-if="connectionConfigJson(row)" variant="text" @click="copyPluginValue(connectionConfigJson(row)!, '连接 JSON')"><template #icon><CopyIcon /></template>复制连接 JSON</t-button><t-button v-if="row.status === 'installed'" variant="text" @click="toggle(row)">停用</t-button><t-button v-else-if="row.status === 'disabled'" variant="text" @click="toggle(row)">启用</t-button><t-popconfirm content="确认卸载插件并删除关联连接吗？" @confirm="removePlugin(row.id)"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm></template></t-space></template>
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
    <t-dialog v-model:visible="detailVisible" width="960px" top="4vh" :close-on-overlay-click="true" :confirm-btn="null" cancel-btn="关闭">
      <template #header><span class="dialog-title">插件详情 · {{ detail?.display_name || detailName }}</span></template>
      <t-loading :loading="detailLoading" show-overlay>
        <div class="plugin-detail-body">
        <t-alert v-if="detailError" theme="error" :message="detailError" />
        <template v-if="detail">
          <t-descriptions :column="2" size="small" bordered class="plugin-detail-meta">
            <t-descriptions-item label="名称">{{ detail.name }}</t-descriptions-item>
            <t-descriptions-item label="版本">{{ detail.version || "—" }}</t-descriptions-item>
            <t-descriptions-item label="类型">{{ detail.plugin_type }}</t-descriptions-item>
            <t-descriptions-item label="语言">{{ detail.language }}</t-descriptions-item>
            <t-descriptions-item label="目录" :span="2">{{ detail.directory }}</t-descriptions-item>
            <t-descriptions-item label="说明" :span="2">{{ detail.description || "—" }}</t-descriptions-item>
          </t-descriptions>

          <div class="plugin-detail-section">
            <p class="plugin-detail-section-title">实现方法</p>
            <div v-if="detail.methods.length === 0" class="plugin-detail-empty">未实现任何方法</div>
            <div v-else class="plugin-dependency-list">
              <t-tag v-for="method in detail.methods" :key="method" variant="light" class="plugin-dependency-item">{{ method }}</t-tag>
            </div>
          </div>

          <div class="plugin-detail-section">
            <p class="plugin-detail-section-title">依赖</p>
            <div v-if="detail.dependencies.length === 0" class="plugin-detail-empty">未声明依赖</div>
            <div v-else class="plugin-dependency-list">
              <t-tag v-for="dependency in detail.dependencies" :key="dependency" variant="light" class="plugin-dependency-item">{{ dependency }}</t-tag>
            </div>
          </div>

          <div class="plugin-detail-section">
            <button type="button" class="plugin-source-toggle" @click="sourceOpen = !sourceOpen">
              <ChevronRightIcon :class="['plugin-source-chevron', { open: sourceOpen }]" />
              <span>源代码</span>
              <small>{{ detailFileList.length }} 个文件</small>
            </button>
            <div v-if="sourceOpen" class="plugin-source-panel">
              <ul class="plugin-source-tree">
                <li v-for="file in detailFileList" :key="file.path">
                  <button type="button" :class="['plugin-source-file', { active: activeFile === file.path }]" @click="previewFile(file.path)">{{ file.path }}</button>
                </li>
              </ul>
              <div class="plugin-source-code">
                <div class="plugin-source-code-header">{{ activeFile || "选择一个文件" }}</div>
                <div class="plugin-source-code-body">
                  <t-alert v-if="fileError" theme="error" :message="fileError" />
                  <p v-else-if="fileLoading" class="plugin-detail-empty">加载中…</p>
                  <p v-else-if="fileContent && fileContent.binary" class="plugin-detail-empty">二进制文件，无法预览</p>
                  <CodeViewer
                    v-else-if="fileContent && fileContent.content !== null"
                    :code="fileContent.content"
                    :notice="fileContent.truncated ? '文件过大，仅显示前 512 KB' : ''"
                  />
                  <p v-else class="plugin-detail-empty">选择一个文件以预览</p>
                </div>
              </div>
            </div>
          </div>
        </template>
        </div>
      </t-loading>
    </t-dialog>
    <t-dialog v-model:visible="zipVisible" header="安装 zip 包插件" width="560px" :close-on-overlay-click="false">
      <div class="plugin-zip-install">
        <p class="plugin-detail-empty">选择插件目录的 zip 包，根目录需包含 plugin.py 或 plugin.mjs，可带 pyproject.toml 或 requirements.txt 声明依赖。</p>
        <input type="file" accept=".zip,application/zip" @change="selectZipFile(($event.target as HTMLInputElement).files?.[0] ?? (undefined as unknown as File))" />
        <p v-if="zipFile" class="plugin-zip-selected">{{ zipFile.name }}</p>
        <t-alert v-if="zipError" theme="error" :message="zipError" />
        <t-alert v-if="zipWarnings.length > 0" theme="warning" :message="zipWarnings.join('；')" />
      </div>
      <template #footer>
        <t-button variant="outline" @click="zipVisible = false">关闭</t-button>
        <t-button theme="primary" :loading="zipInstalling" :disabled="!zipFile" @click="installZip">开始安装</t-button>
      </template>
    </t-dialog>
    <t-dialog v-model:visible="uninstallCommandVisible" header="卸载命令" :confirm-btn="null" cancel-btn="关闭">
      <div class="plugin-command-dialog">
        <t-button variant="text" shape="square" @click="copyPluginValue(uninstallCommand, '卸载命令')"><template #icon><CopyIcon /></template></t-button>
        <t-textarea :value="uninstallCommand" readonly autosize />
      </div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { ChevronLeftIcon, ChevronRightIcon, CopyIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import CodeViewer from "../components/CodeViewer.vue";
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
  isScriptPlugin,
  installMethodLabel,
  componentLabel,
  scriptPluginDetail,
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
  detailVisible,
  detailLoading,
  detail,
  detailError,
  sourceOpen,
  activeFile,
  fileContent,
  fileLoading,
  fileError,
  detailFileList,
  openDetail,
  previewFile,
  removeScriptPlugin,
  zipVisible,
  zipFile,
  zipInstalling,
  zipError,
  zipWarnings,
  openZipInstall,
  selectZipFile,
  installZip,
} = usePluginsPage();
</script>

<style scoped lang="scss">
@use "./plugins" as *;
</style>
