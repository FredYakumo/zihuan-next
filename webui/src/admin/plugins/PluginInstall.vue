<template>
  <section class="page plugins-page plugin-install-page">
    <AdminPageHeader :breadcrumbs="breadcrumbs">
      <t-button v-if="installStep === 'config'" variant="outline" @click="backFromConfig">返回选择组件</t-button>
      <t-button variant="outline" @click="requestCloseInstall">返回列表</t-button>
    </AdminPageHeader>

    <t-card class="plugin-install-card" bordered>
      <PluginInstallPanel :page="page" />
    </t-card>

    <div class="plugin-install-footer">
      <t-button variant="outline" :disabled="installStep === 'progress' && installDone" @click="requestCloseInstall">
        取消
      </t-button>
      <t-button
        v-if="installStep !== 'progress'"
        theme="primary"
        :loading="saving"
        :disabled="installStep !== 'config' || (!isEmbeddedComponent(form.component_type) && environmentLoading)"
        @click="install"
      >
        开始安装
      </t-button>
      <t-button v-else-if="installDone" theme="primary" @click="confirmInstallResult">确认</t-button>
    </div>

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
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted } from "vue";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import InstallationSuccessDialog from "../components/setup/InstallationSuccessDialog.vue";
import PluginInstallPanel from "./PluginInstallPanel.vue";
import { usePluginsPage } from "./Plugins";

const page = usePluginsPage({ autoLoad: false });
const {
  form,
  installStep,
  installDone,
  saving,
  environmentLoading,
  exitConfirmVisible,
  successVisible,
  successInstallCommand,
  successConnectionConfig,
  openInstall,
  install,
  requestCloseInstall,
  backFromConfig,
  confirmExitInstallation,
  confirmInstallResult,
  confirmInstallationSuccess,
  isEmbeddedComponent,
} = page;

const breadcrumbs = computed(() => [{ label: "插件", to: "/plugins" }, { label: "安装插件" }]);

onMounted(() => {
  openInstall();
});
</script>

<style scoped lang="scss">
@use "./plugins" as *;
</style>
