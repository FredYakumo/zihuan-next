<template>
  <section class="page plugins-page">
    <AdminPageHeader title="插件">
      <t-button theme="primary" @click="openZipInstall">安装 zip 包</t-button>
    </AdminPageHeader>
    <t-alert v-if="error" theme="error" :message="error" />
    <t-card bordered>
      <t-table :data="plugins" :columns="columns" row-key="id" :loading="loading">
        <template #actions="{ row }">
          <t-space>
            <t-button variant="text" @click="openDetail(row)">详情</t-button>
            <t-popconfirm content="确认卸载插件？" @confirm="removeScriptPlugin(row)">
              <t-button variant="text" theme="danger">卸载</t-button>
            </t-popconfirm>
          </t-space>
        </template>
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

  </section>
</template>

<script setup lang="ts">
import { ChevronRightIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import CodeViewer from "../components/CodeViewer.vue";
import { usePluginsPage } from "./Plugins";

const {
  plugins, loading, error, columns, detailVisible, detailLoading, detail,
  detailName, detailError, sourceOpen, activeFile, fileContent, fileLoading,
  fileError, detailFileList, openDetail, previewFile, removeScriptPlugin,
  zipVisible, zipFile, zipInstalling, zipError, zipWarnings, openZipInstall,
  selectZipFile, installZip,
} = usePluginsPage();
</script>

<style scoped lang="scss">
@use "./plugins" as *;
</style>
