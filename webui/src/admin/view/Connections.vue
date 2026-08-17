<template>
  <section class="page connections-page">
    <AdminPageHeader title="连接配置">
      <t-button variant="outline" @click="triggerImportFile">导入配置</t-button>
      <input ref="importFileInput" type="file" accept=".json" class="connection-import-input" @change="handleFileChange" />
      <t-button theme="primary" @click="openCreateChoice">新建连接</t-button>
    </AdminPageHeader>

    <t-card class="connections-card" bordered>
      <div class="connections-toolbar">
        <t-input v-model="filters.keyword" clearable placeholder="搜索名称、类型或 Config ID" />
        <t-select v-model="filters.type">
          <t-option value="all" label="全部类型" />
          <t-option v-for="type in connectionTypes" :key="type.value" :value="type.value" :label="type.label" />
        </t-select>
        <t-select v-model="filters.enabled">
          <t-option value="all" label="全部状态" />
          <t-option value="enabled" label="已启用" />
          <t-option value="disabled" label="已停用" />
        </t-select>
        <t-button variant="text" @click="load">刷新</t-button>
        <span class="connections-count">共 {{ filteredConnections.length }} 条</span>
      </div>

      <t-table
        row-key="config_id"
        :data="filteredConnections"
        :columns="columns"
        :hover="true"
        :pagination="false"
        table-layout="fixed"
      >
        <template #name="{ row }">
          <div class="connection-name-cell">
            <strong>{{ row.name }}</strong>
            <span class="mono">{{ compactId(row.config_id) }}</span>
          </div>
        </template>
        <template #type="{ row }">
          <t-tag variant="light">{{ connectionTypeLabel(row.kind.type) }}</t-tag>
        </template>
        <template #enabled="{ row }">
          <t-tag variant="light" :theme="row.enabled ? 'success' : 'warning'">
            {{ row.enabled ? "已启用" : "已停用" }}
          </t-tag>
        </template>
        <template #summary="{ row }">
          <span class="connection-summary" :title="connectionSummary(row)">{{ connectionSummary(row) || "—" }}</span>
        </template>
        <template #runtime="{ row }">
          <span>{{ runtimeInstanceCount(row.config_id) }} 个实例</span>
        </template>
        <template #updated_at="{ row }">{{ formatTime(row.updated_at) }}</template>
        <template #actions="{ row }">
          <div class="connection-actions">
            <t-button variant="text" size="small" @click="editConnection(row)">编辑</t-button>
            <t-button variant="text" size="small" @click="duplicateConnection(row)">复制添加</t-button>
            <t-button variant="text" size="small" @click="copyConnectionConfig(row)">
              {{ copiedId === row.config_id ? "已复制" : "复制" }}
            </t-button>
            <t-popconfirm content="确认删除这个连接配置吗？" @confirm="removeConnection(row.config_id)">
              <t-button variant="text" theme="danger" size="small">删除</t-button>
            </t-popconfirm>
          </div>
        </template>
        <template #empty>
          <div class="connections-empty">暂无匹配的连接配置。</div>
        </template>
      </t-table>
    </t-card>

    <t-dialog v-model:visible="createChoiceVisible" header="新建连接" :footer="false">
      <div class="create-choice-grid">
        <button class="create-choice" @click="chooseExistingConnection"><strong>现有连接</strong><span>使用当前连接配置逻辑</span></button>
        <button class="create-choice" @click="choosePluginConnection"><strong>安装插件</strong><span>安装基础设施并自动创建连接</span></button>
      </div>
    </t-dialog>

    <t-drawer
      v-model:visible="drawerVisible"
      :header="isCreating ? '新建连接' : '编辑连接'"
      size="640px"
      :close-on-overlay-click="false"
      @close="closeDrawer"
    >
      <t-form class="connection-form" label-align="top">
        <div class="connection-form-section">
          <h3>基本信息</h3>
          <div class="connection-form-grid">
            <t-form-item label="名称" required>
              <t-input v-model="form.name" placeholder="请输入连接名称" />
            </t-form-item>
            <t-form-item label="连接类型" required>
              <t-select v-model="form.type" :disabled="!isCreating">
                <t-option v-for="type in connectionTypes" :key="type.value" :value="type.value" :label="type.label" />
              </t-select>
            </t-form-item>
          </div>
          <t-checkbox v-model="form.enabled">启用该连接</t-checkbox>
        </div>

        <div class="connection-form-section">
          <h3>{{ connectionTypeLabel(form.type) }} 配置</h3>
          <div class="connection-form-grid">
            <template v-if="form.type === 'mysql'">
              <t-form-item label="地址" required><t-input v-model="form.mysql_host" placeholder="127.0.0.1" /></t-form-item>
              <t-form-item label="端口" required><t-input v-model="form.mysql_port" placeholder="3306" /></t-form-item>
              <t-form-item label="账号（可选）"><t-input v-model="form.mysql_user" /></t-form-item>
              <t-form-item label="密码（可选）"><ConnectionCredentialInput v-model="form.mysql_password" placeholder="请输入密码" /></t-form-item>
              <t-form-item class="connection-form-item--full" label="数据库名" required><t-input v-model="form.mysql_database" placeholder="zihuan" /></t-form-item>
              <t-form-item label="最大连接数"><t-input-number v-model="form.mysql_max_connections" :min="1" /></t-form-item>
              <t-form-item label="获取连接超时（秒）"><t-input-number v-model="form.mysql_acquire_timeout_secs" :min="1" /></t-form-item>
            </template>

            <template v-else-if="form.type === 'redis'">
              <t-form-item class="connection-form-item--full" label="URL"><t-input v-model="form.redis_url" placeholder="redis://127.0.0.1:6379" /></t-form-item>
              <t-form-item label="用户名（可选）"><t-input v-model="form.redis_username" placeholder="default" /></t-form-item>
              <t-form-item label="密码（可选）"><ConnectionCredentialInput v-model="form.redis_password" /></t-form-item>
            </template>

            <template v-else-if="form.type === 'weaviate'">
              <t-form-item label="Base URL" required><t-input v-model="form.weaviate_base_url" /></t-form-item>
              <t-form-item label="Class Name" required><t-input v-model="form.weaviate_class_name" /></t-form-item>
              <t-form-item label="认证方式"><t-select v-model="form.weaviate_auth_method" @change="clearInactiveConnectionCredentials('weaviate')"><t-option value="password" label="密码" /><t-option value="api_key" label="API Key" /></t-select></t-form-item>
              <t-form-item v-if="form.weaviate_auth_method === 'password'" label="用户名" required><t-input v-model="form.weaviate_username" /></t-form-item>
              <t-form-item :label="form.weaviate_auth_method === 'password' ? '密码' : 'API Key'" required><ConnectionCredentialInput v-if="form.weaviate_auth_method === 'password'" v-model="form.weaviate_password" /><ConnectionCredentialInput v-else v-model="form.weaviate_api_key" /></t-form-item>
              <t-form-item label="Collection Schema"><t-select v-model="form.weaviate_collection_schema"><t-option value="image_semantic" label="图片语义" /><t-option value="agent_memory" label="Agent 记忆" /></t-select></t-form-item>
            </template>

            <template v-else-if="form.type === 'elasticsearch'">
              <t-form-item label="Base URL" required><t-input v-model="form.elasticsearch_base_url" placeholder="https://localhost:9200" /></t-form-item>
              <t-form-item label="Index Name" required><t-input v-model="form.elasticsearch_index_name" /></t-form-item>
              <t-form-item label="认证方式"><t-select v-model="form.elasticsearch_auth_method" @change="clearInactiveConnectionCredentials('elasticsearch')"><t-option value="password" label="密码" /><t-option value="api_key" label="API Key" /></t-select></t-form-item>
              <t-form-item v-if="form.elasticsearch_auth_method === 'password'" label="用户名" required><t-input v-model="form.elasticsearch_username" /></t-form-item>
              <t-form-item :label="form.elasticsearch_auth_method === 'password' ? '密码' : 'API Key'" required><ConnectionCredentialInput v-if="form.elasticsearch_auth_method === 'password'" v-model="form.elasticsearch_password" /><ConnectionCredentialInput v-else v-model="form.elasticsearch_api_key" /></t-form-item>
              <t-form-item label="索引用途"><t-select v-model="form.elasticsearch_collection_schema"><t-option value="agent_memory" label="Agent 记忆" /><t-option value="image_semantic" label="图片语义" /></t-select></t-form-item>
              <t-form-item label="向量维度"><t-input-number v-model="form.elasticsearch_vector_dimensions" :min="1" /></t-form-item>
            </template>

            <template v-else-if="form.type === 'rustfs'">
              <t-form-item label="Endpoint"><t-input v-model="form.rustfs_endpoint" /></t-form-item>
              <t-form-item label="Bucket"><t-input v-model="form.rustfs_bucket" /></t-form-item>
              <t-form-item label="Region"><t-input v-model="form.rustfs_region" /></t-form-item>
              <t-form-item label="Access Key"><t-input v-model="form.rustfs_access_key" /></t-form-item>
              <t-form-item label="Secret Key"><ConnectionCredentialInput v-model="form.rustfs_secret_key" /></t-form-item>
              <t-form-item label="Public Base URL"><t-input v-model="form.rustfs_public_base_url" /></t-form-item>
              <t-form-item class="connection-form-item--full"><t-checkbox v-model="form.rustfs_path_style">使用 path-style</t-checkbox></t-form-item>
            </template>

            <template v-else-if="isBotAdapterConnectionType(form.type)">
              <t-form-item label="Bot WS URL"><t-input v-model="form.bot_server_url" placeholder="ws://192.168.71.2:3008" /></t-form-item>
              <t-form-item label="Adapter HTTP URL"><t-input v-model="form.adapter_server_url" placeholder="http://192.168.71.2:3001" /></t-form-item>
              <t-form-item label="QQ 号"><t-input v-model="form.qq_id" /></t-form-item>
              <t-form-item label="Token"><ConnectionCredentialInput v-model="form.bot_server_token" /></t-form-item>
            </template>

            <template v-else-if="form.type === 'web_search_engine'">
              <t-form-item label="Provider"><t-select v-model="form.web_search_engine_provider"><t-option value="tavily" label="Tavily" /><t-option value="brave" label="Brave" /></t-select></t-form-item>
              <t-form-item label="Timeout（秒）"><t-input-number v-model="form.web_search_engine_timeout_secs" :min="1" /></t-form-item>
              <t-form-item class="connection-form-item--full" label="API Token（可选）"><ConnectionCredentialInput v-model="form.web_search_engine_api_token" /></t-form-item>
            </template>

            <template v-else-if="form.type === 'tokenizer'">
              <t-form-item class="connection-form-item--full" label="Tokenizer 模型" required><t-select v-model="form.tokenizer_model_name" placeholder="请选择"><t-option v-for="model in tokenizerModels" :key="model" :value="model" :label="model" /></t-select></t-form-item>
            </template>

            <template v-else-if="form.type === 'sqlite'">
              <t-form-item class="connection-form-item--full" label="数据库文件路径"><t-input v-model="form.sqlite_path" placeholder="/path/to/database.db" /></t-form-item>
            </template>
          </div>
        </div>
      </t-form>

      <template #footer>
        <div class="connection-drawer-footer">
          <t-button variant="outline" @click="closeDrawer">取消</t-button>
          <t-button theme="primary" @click="submitForm">{{ isCreating ? "创建连接" : "保存修改" }}</t-button>
        </div>
      </template>
    </t-drawer>
  </section>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRoute, useRouter } from "vue-router";

import type { ConnectionConfig } from "../../api/client";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { compactId } from "../model";
import { useConnections } from "../composables/useConnections";
import ConnectionCredentialInput from "./ConnectionCredentialInput.vue";

const {
  filteredConnections,
  tokenizerModels,
  form,
  drawerVisible,
  isCreating,
  filters,
  connectionTypes,
  startCreate,
  closeDrawer,
  load,
  editConnection,
  duplicateConnection,
  submitForm,
  removeConnection,
  connectionSummary,
  runtimeInstanceCount,
  clearInactiveConnectionCredentials,
  formatTime,
  isBotAdapterConnectionType,
  copiedId,
  copyConfig,
  handleFileChange,
} = useConnections();

const importFileInput = ref<HTMLInputElement | null>(null);
const route = useRoute();
const router = useRouter();
const createChoiceVisible = ref(false);

function openCreateChoice() { createChoiceVisible.value = true; }
if (route.query.action === "create") openCreateChoice();
function chooseExistingConnection() { createChoiceVisible.value = false; startCreate(); }
function choosePluginConnection() { createChoiceVisible.value = false; router.push("/plugins?install=1"); }

const columns = [
  { colKey: "name", title: "连接名称", width: 210 },
  { colKey: "type", title: "类型", width: 130 },
  { colKey: "enabled", title: "状态", width: 100 },
  { colKey: "summary", title: "连接摘要", ellipsis: true },
  { colKey: "runtime", title: "运行时", width: 100 },
  { colKey: "updated_at", title: "更新时间", width: 170 },
  { colKey: "actions", title: "操作", width: 270, fixed: "right" },
];

function triggerImportFile() {
  importFileInput.value?.click();
}

function connectionTypeLabel(type: string): string {
  return connectionTypes.find((option) => option.value === type)?.label ?? type;
}

function copyConnectionConfig(connection: ConnectionConfig) {
  copyConfig({ name: connection.name, enabled: connection.enabled, kind: connection.kind }, connection.config_id);
}
</script>

<style scoped lang="scss">
@use "../styles/connections" as *;

.create-choice-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.create-choice { display: grid; gap: 8px; padding: 20px; text-align: left; border: 1px solid var(--admin-border); background: var(--admin-surface); border-radius: 8px; cursor: pointer; color: inherit; }
.create-choice:hover { border-color: var(--td-brand-color); }
.create-choice span { color: var(--admin-muted); font-size: 13px; }
</style>
