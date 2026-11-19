<template>
  <section class="page plugins-page">
    <AdminPageHeader title="插件" />
    <t-card title="已安装插件" bordered header-bordered>
      <template #actions><t-space size="small"><t-button variant="outline" @click="openZipInstall">安装 zip 包</t-button><t-button theme="primary" @click="router.push('/plugins/install')">安装插件</t-button></t-space></template>
      <t-table :data="plugins" :columns="columns" row-key="id" :loading="loading" :pagination="false" size="small">
        <template #name="{ row }"><div class="plugin-name-cell"><span>{{ row.name }}</span><small v-if="scriptPluginDetail(row)" class="plugin-name-desc" :title="scriptPluginDetail(row)">{{ scriptPluginDetail(row) }}</small><small v-if="wechatWxid(row)" class="plugin-wxid-line"><span>wxid: {{ wechatWxid(row) }}</span><button type="button" class="plugin-wxid-copy" title="复制 wxid" @click="copyPluginValue(wechatWxid(row), 'wxid')"><CopyIcon /></button></small></div></template>
        <template #component_type="{ row }"><span :title="row.component_type">{{ componentLabel(row) }}</span></template>
        <template #installation_method="{ row }"><span>{{ installMethodLabel(row.installation_method) }}</span></template>
        <template #status="{ row }"><t-tag :theme="statusTheme(row.status)" variant="light" :title="statusTitle(row)">{{ statusLabel(row.status) }}</t-tag></template>
        <template #connection_ids="{ row }"><span v-if="isScriptPlugin(row)">—</span><span v-else>{{ row.connection_ids?.length ?? 0 }} 个连接</span></template>
        <template #actions="{ row }"><t-space size="small"><template v-if="isScriptPlugin(row)"><t-button variant="text" @click="openDetail(row)">详情</t-button><t-popconfirm content="确认卸载插件并删除其目录吗？" @confirm="removeScriptPlugin(row)"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm></template><template v-else><t-button v-if="wechatWxid(row)" variant="text" theme="success" :title="'已登录 ' + wechatWxid(row) + '，点击复制 wxid'" @click="copyPluginValue(wechatWxid(row), 'wxid')"><template #icon><CopyIcon /></template>复制 wxid</t-button><t-button v-if="row.component_type === 'wechat_pad' && ['installed', 'command_generated', 'disabled'].includes(row.status)" variant="text" @click="openWechatLogin(row)">扫码登录</t-button><t-button v-if="webuiUrl(row)" variant="text" title="打开 NapCat WebUI 登录 QQ" @click="openWebui(row)">打开 WebUI</t-button><t-button v-if="row.component_type === 'napcat' && webuiUrl(row)" variant="text" title="查看 WebUI 地址与登录 Token" @click="openNapcatConfig(row)">打开配置</t-button><t-button v-if="installCommand(row)" variant="text" @click="copyPluginValue(installCommand(row)!, '安装命令')"><template #icon><CopyIcon /></template>复制安装命令</t-button><t-button v-if="connectionConfigJson(row)" variant="text" @click="copyPluginValue(connectionConfigJson(row)!, '连接 JSON')"><template #icon><CopyIcon /></template>复制连接 JSON</t-button><t-button v-if="row.status === 'installed'" variant="text" @click="toggle(row)">停用</t-button><t-button v-else-if="row.status === 'disabled'" variant="text" @click="toggle(row)">启用</t-button><t-popconfirm content="确认卸载插件并删除关联连接吗？" @confirm="removePlugin(row)"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm></template></t-space></template>
      </t-table>
    </t-card>
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
    <t-dialog
      v-model:visible="wechatLoginVisible"
      header="微信扫码登录"
      width="420px"
      :close-on-overlay-click="false"
      :confirm-btn="null"
      cancel-btn="关闭"
      @close="closeWechatLogin"
    >
      <div class="wechat-login-body">
        <t-loading :loading="wechatLoginLoading" show-overlay>
          <div v-if="wechatLoginQrBase64 || wechatLoginQrLink" class="wechat-login-qr">
            <img v-if="wechatLoginQrBase64" :src="wechatLoginQrBase64" alt="微信登录二维码" />
            <a v-else :href="wechatLoginQrLink" target="_blank">打开登录链接</a>
          </div>
          <div v-else-if="wechatLoginPhase === 'logged_in'" class="wechat-login-success">
            <p>✅ 登录成功</p>
            <p v-if="wechatLoginAccount?.nickname || wechatLoginAccount?.wxid" class="wechat-login-account">
              {{ wechatLoginAccount?.nickname || wechatLoginAccount?.wxid }}<span v-if="wechatLoginAccount?.wxid">（{{ wechatLoginAccount.wxid }}）</span>
            </p>
            <p class="wechat-login-hint">授权码与账号 wxid 已写回连接配置。</p>
          </div>
          <div v-else class="wechat-login-empty">正在获取二维码…</div>
        </t-loading>
        <p :class="['wechat-login-status', { error: wechatLoginPhase === 'error' }]">{{ wechatLoginStatusLabel() }}</p>
        <t-button v-if="wechatLoginPhase === 'expired' || wechatLoginPhase === 'error'" variant="outline" @click="requestWechatQr">刷新二维码</t-button>
      </div>
    </t-dialog>
    <t-dialog
      v-model:visible="napcatConfigVisible"
      header="NapCat 配置"
      width="600px"
      :close-on-overlay-click="true"
      :confirm-btn="null"
      cancel-btn="关闭"
    >
      <div v-if="napcatConfigPlugin" class="napcat-config-list">
        <div class="napcat-config-item">
          <div class="napcat-config-info">
            <small>WebUI 地址</small>
            <code>{{ webuiUrl(napcatConfigPlugin) }}</code>
          </div>
          <t-button v-if="webuiUrl(napcatConfigPlugin)" variant="text" size="small" title="复制 WebUI 地址" @click="copyPluginValue(webuiUrl(napcatConfigPlugin)!, 'WebUI 地址')"><template #icon><CopyIcon /></template></t-button>
        </div>
        <div v-if="webuiToken(napcatConfigPlugin)" class="napcat-config-item">
          <div class="napcat-config-info">
            <small>WebUI 登录 Token</small>
            <SetupCredentialInput v-model="napcatConfigToken" readonly />
          </div>
          <t-button variant="text" size="small" title="复制 WebUI 登录 Token" @click="copyPluginValue(webuiToken(napcatConfigPlugin)!, 'WebUI 登录 Token')"><template #icon><CopyIcon /></template></t-button>
        </div>
        <div v-if="wsUrl(napcatConfigPlugin)" class="napcat-config-item">
          <div class="napcat-config-info">
            <small>OneBot WebSocket 地址</small>
            <code>{{ wsUrl(napcatConfigPlugin) }}</code>
          </div>
          <t-button v-if="wsUrl(napcatConfigPlugin)" variant="text" size="small" title="复制 WebSocket 地址" @click="copyPluginValue(wsUrl(napcatConfigPlugin)!, 'OneBot WebSocket 地址')"><template #icon><CopyIcon /></template></t-button>
        </div>
        <t-alert
          v-if="webuiToken(napcatConfigPlugin)"
          theme="info"
          message="Token 在容器首次初始化时写入；若重装容器后不匹配，请清除 NapCat 配置目录后重新安装。"
        />
        <t-alert
          v-else
          theme="info"
          message="native 方式安装的 NapCat Token 由本体生成，请在其控制台或 WebUI 登录页查看。"
        />
      </div>
    </t-dialog>
    <t-dialog
      v-model:visible="uninstallProgressVisible"
      :header="uninstallFailed ? '卸载失败' : uninstallDone ? '卸载完成' : '正在卸载插件...'"
      width="640px"
      :close-on-overlay-click="false"
      :confirm-btn="uninstallFailed || uninstallDone ? '确定' : null"
      :cancel-btn="uninstallFailed || uninstallDone ? null : '关闭'"
      @close="closeUninstallProgress"
      @confirm="confirmUninstallResult"
    >
      <div class="uninstall-console">
        <div v-for="(event, i) in uninstallProgressEvents" :key="i" class="uninstall-line" :class="event.status">
          <span class="uninstall-step">[{{ event.step }}]</span>
          <span class="uninstall-message">{{ event.message }}</span>
        </div>
        <div v-if="uninstallProgressEvents.length === 0" class="uninstall-line running"><span class="uninstall-message">等待任务启动...</span></div>
      </div>
      <div v-if="uninstallFailed" class="uninstall-failure">
        <p class="uninstall-command-hint">自动卸载未完成。请复制以下命令手动执行清理，然后重试卸载：</p>
        <t-alert v-if="uninstallError" theme="error" :message="uninstallError" />
        <div class="plugin-command-dialog">
          <t-button class="plugin-command-copy" variant="text" shape="square" @click="copyPluginValue(uninstallCommandText, '卸载命令')"><template #icon><CopyIcon /></template></t-button>
          <t-textarea :value="uninstallCommandText" readonly autosize />
        </div>
      </div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { ChevronRightIcon, CopyIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import CodeViewer from "../components/CodeViewer.vue";
import SetupCredentialInput from "../components/setup/SetupCredentialInput.vue";
import { usePluginsPage } from "./Plugins";

const {
  router,
  plugins,
  loading,
  uninstallProgressVisible,
  uninstallProgressEvents,
  uninstallFailed,
  uninstallDone,
  uninstallError,
  uninstallCommandText,
  closeUninstallProgress,
  confirmUninstallResult,
  columns,
  statusLabel,
  statusTheme,
  statusTitle,
  isScriptPlugin,
  installMethodLabel,
  componentLabel,
  scriptPluginDetail,
  wechatWxid,
  toggle,
  installCommand,
  connectionConfigJson,
  webuiUrl,
  webuiToken,
  wsUrl,
  openWebui,
  napcatConfigVisible,
  napcatConfigPlugin,
  napcatConfigToken,
  openNapcatConfig,
  copyPluginValue,
  removePlugin,
  detailVisible,
  detailLoading,
  detail,
  detailName,
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
  wechatLoginVisible,
  wechatLoginLoading,
  wechatLoginPhase,
  wechatLoginError,
  wechatLoginQrBase64,
  wechatLoginQrLink,
  wechatLoginAccount,
  openWechatLogin,
  closeWechatLogin,
  requestWechatQr,
  wechatLoginStatusLabel,
} = usePluginsPage();
</script>

<style scoped lang="scss">
@use "./plugins" as *;
</style>
