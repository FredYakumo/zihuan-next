<template>
  <t-dialog
    :visible="visible"
    header="新增检索数据库"
    :confirm-btn="null"
    cancel-btn="关闭"
    :close-on-overlay-click="false"
    width="760px"
    @update:visible="emit('update:visible', $event)"
  >
    <div class="retrieval-database-dialog">
      <div class="mode-switch">
        <label><input v-model="mode" type="radio" value="install" /> 安装新数据库</label>
        <label><input v-model="mode" type="radio" value="existing" /> 使用现有配置</label>
      </div>

      <template v-if="mode === 'install'">
        <p v-if="environmentLoading" class="install-hint">正在检测本机安装能力...</p>
        <template v-else>
          <div class="choice-row">
            <span class="choice-title">数据库类型</span>
            <label><input v-model="searchType" type="radio" value="weaviate" /> Weaviate</label>
            <label><input v-model="searchType" type="radio" value="elasticsearch" /> Elasticsearch</label>
          </div>
          <div class="choice-row">
            <span class="choice-title">安装方式</span>
            <label :class="{ unavailable: !dockerSupported }" :title="dockerUnsupportedReason">
              <input v-model="installOption" type="radio" value="local_docker" :disabled="!dockerSupported" />
              本机 Docker <small v-if="!dockerSupported">（本机不支持）</small>
            </label>
            <label
              :class="{ unavailable: !localBinarySupported }"
              :title="localBinaryDisabledReason ?? ''"
            >
              <input v-model="installOption" type="radio" value="local_binary" :disabled="!localBinarySupported" />
              本机二进制 <small v-if="localBinaryDisabledReason">（{{ localBinaryDisabledReason }}）</small>
            </label>
            <label>
              <input v-model="installOption" type="radio" value="command_docker" />
              安装命令（Docker）
            </label>
            <label>
              <input v-model="installOption" type="radio" value="command_binary" />
              安装命令（二进制）
            </label>
          </div>

          <div class="install-grid">
            <label class="install-grid-item">
              <span>镜像</span>
              <input v-model="search.deployment.image" />
            </label>
            <label class="install-grid-item">
              <span>端口</span>
              <input v-model.number="search.deployment.port" type="number" min="1" />
            </label>
            <label class="install-grid-item">
              <span>数据目录</span>
              <input v-model="search.deployment.data_dir" />
            </label>
            <label class="install-grid-item">
              <span>容器名</span>
              <input v-model="search.deployment.container_name" />
            </label>
            <label class="install-grid-item">
              <span>服务器地址</span>
              <input
                v-model="serverHost"
                :disabled="isLocalOption"
                placeholder="例如 192.168.1.10 或 server.example.com"
              />
            </label>
            <label v-if="search.auth_method === 'api_key'" class="install-grid-item">
              <span>API Key</span>
              <input v-model="search.api_key" />
            </label>
            <template v-else>
              <label class="install-grid-item">
                <span>用户名</span>
                <input v-model="search.username" disabled />
              </label>
              <label class="install-grid-item">
                <span>密码</span>
                <input v-model="search.password" type="password" />
              </label>
            </template>
            <label v-if="search.type === 'elasticsearch'" class="install-grid-item">
              <span>向量维度</span>
              <input v-model.number="search.vector_dimensions" type="number" min="1" />
            </label>
          </div>

          <p class="install-hint">连接配置将使用 {{ resolvedBaseUrl }}</p>

          <p v-if="installError" class="install-error">{{ installError }}</p>

          <div class="install-actions">
            <t-button v-if="isLocalOption" theme="primary" :loading="installing" @click="startInstall">
              {{ installing ? "安装中..." : "开始安装" }}
            </t-button>
            <t-button v-else theme="primary" :loading="generating" @click="generateCommand">
              生成安装命令
            </t-button>
            <t-button v-if="installError || installLogs.length > 0" variant="text" :disabled="installing" @click="resetInstallState">
              重置
            </t-button>
          </div>

          <div v-if="installLogs.length > 0" class="install-progress">
            <div v-for="(line, index) in installLogs" :key="index" class="install-progress-line">{{ line }}</div>
          </div>

          <template v-if="commandResult">
            <section class="command-output">
              <div class="command-output-header">
                <span>安装命令</span>
                <t-button variant="text" size="small" @click="copyText(commandResult.install_command, 'command')">
                  <CopyIcon /> {{ copied === "command" ? "已复制" : "复制" }}
                </t-button>
              </div>
              <textarea readonly :value="commandResult.install_command" aria-label="安装命令" />
            </section>
            <section class="command-output">
              <div class="command-output-header">
                <span>连接配置 JSON</span>
                <t-button variant="text" size="small" @click="copyText(connectionsJson, 'connections')">
                  <CopyIcon /> {{ copied === "connections" ? "已复制" : "复制" }}
                </t-button>
              </div>
              <textarea readonly :value="connectionsJson" aria-label="连接配置 JSON" />
            </section>
            <div class="install-actions">
              <t-button theme="primary" :loading="importingCommand" @click="importCommandConnection">
                已完成安装，导入连接
              </t-button>
            </div>
          </template>
        </template>
      </template>

      <template v-else>
        <div class="existing-actions">
          <t-button block theme="primary" @click="emit('create-page')">前往连接页创建</t-button>
          <t-button block variant="outline" @click="emit('clipboard-import')">从剪贴板导入</t-button>
          <t-button block variant="outline" @click="fileInput?.click()">从 JSON 导入</t-button>
          <input
            ref="fileInput"
            type="file"
            accept=".json,application/json"
            class="existing-file-input"
            @change="emit('file-change', $event)"
          />
        </div>
      </template>
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { CopyIcon } from "tdesign-icons-vue-next";

import type { ConnectionConfig } from "../../api/client";
import { useRetrievalDatabaseDialog } from "./RetrievalDatabaseDialog";

const props = defineProps<{ visible: boolean }>();

const emit = defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "created", connection: ConnectionConfig): void;
  (event: "clipboard-import"): void;
  (event: "file-change", value: Event): void;
  (event: "create-page"): void;
}>();

const {
  mode,
  searchType,
  search,
  installOption,
  environmentLoading,
  installError,
  installLogs,
  commandResult,
  copied,
  fileInput,
  serverHost,
  dockerSupported,
  dockerUnsupportedReason,
  localBinarySupported,
  localBinaryDisabledReason,
  isLocalOption,
  resolvedBaseUrl,
  connectionsJson,
  installing,
  generating,
  importingCommand,
  startInstall,
  generateCommand,
  resetInstallState,
  importCommandConnection,
  copyText,
} = useRetrievalDatabaseDialog(props, emit);
</script>

<style scoped lang="scss">
@use "./retrieval-database-dialog" as *;
</style>
