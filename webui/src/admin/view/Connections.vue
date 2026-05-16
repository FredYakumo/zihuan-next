<template>
  <section class="page">
    <div class="page-hero">
      <h2>连接配置</h2>
      <div class="hero-actions connection-hero-actions">
        <button class="btn primary connection-hero-add-btn" @click="startCreate">+</button>
      </div>
    </div>

    <div v-if="showCreatePicker" class="connection-picker-backdrop">
      <div class="connection-picker-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>{{ showEditor && !form.id ? "新建连接" : "选择连接类型" }}</h3>
          <button class="btn ghost connection-card-compact-btn" @click="closeCreatePicker">{{ showEditor && !form.id ? "关闭" : "取消" }}</button>
        </div>
        <div v-if="showEditor && !form.id" class="connection-picker-form">
          <div class="form-grid">
            <div class="field">
              <label>名称</label>
              <input v-model="form.name" />
            </div>
            <div class="field">
              <label>类型</label>
              <select v-model="form.type">
                <option value="mysql">MySQL</option>
                <option value="redis">Redis</option>
                <option value="weaviate">Weaviate</option>
                <option value="rustfs">RustFS</option>
                <option value="bot_adapter">Bot Adapter</option>
                <option value="tavily">Tavily</option>
              </select>
            </div>

            <div class="field-full field-check">
              <input id="connection-enabled" v-model="form.enabled" type="checkbox" />
              <label for="connection-enabled">启用该连接</label>
            </div>

            <template v-if="form.type === 'mysql'">
              <div class="field">
                <label>地址</label>
                <input v-model="form.mysql_host" placeholder="127.0.0.1" />
              </div>
              <div class="field">
                <label>端口</label>
                <input v-model="form.mysql_port" placeholder="3306" />
              </div>
              <div class="field">
                <label>账号（可选）</label>
                <input v-model="form.mysql_user" placeholder="可选" />
              </div>
              <div class="field">
                <label>密码（可选）</label>
                <input v-model="form.mysql_password" type="password" placeholder="请输入密码" />
              </div>
              <div class="field-full">
                <label>数据库名</label>
                <input v-model="form.mysql_database" placeholder="zihuan" />
              </div>
              <div class="field">
                <label>最大连接数</label>
                <input v-model.number="form.mysql_max_connections" type="number" min="1" step="1" />
              </div>
              <div class="field">
                <label>获取连接超时（秒）</label>
                <input v-model.number="form.mysql_acquire_timeout_secs" type="number" min="1" step="1" />
              </div>
            </template>

            <template v-else-if="form.type === 'redis'">
              <div class="field-full">
                <label>URL</label>
                <input v-model="form.redis_url" placeholder="redis://127.0.0.1:6379" />
              </div>
              <div class="field">
                <label>用户名（可选）</label>
                <input v-model="form.redis_username" placeholder="default" />
              </div>
              <div class="field">
                <label>密码（可选）</label>
                <input v-model="form.redis_password" type="password" placeholder="可选" />
              </div>
            </template>

            <template v-else-if="form.type === 'weaviate'">
              <div class="field">
                <label>Base URL</label>
                <input v-model="form.weaviate_base_url" />
              </div>
              <div class="field">
                <label>Class Name</label>
                <input v-model="form.weaviate_class_name" />
              </div>
              <div class="field">
                <label>用户名（可选）</label>
                <input v-model="form.weaviate_username" placeholder="可选" />
              </div>
              <div class="field">
                <label>密码（可选）</label>
                <input v-model="form.weaviate_password" type="password" placeholder="可选" />
              </div>
              <div class="field-full">
                <label>API Key（可选）</label>
                <input v-model="form.weaviate_api_key" type="password" placeholder="可选" />
              </div>
              <div class="field-full">
                <label>Collection Schema</label>
                <select v-model="form.weaviate_collection_schema">
                  <option value="message_record_semantic">消息记录语义</option>
                  <option value="image_semantic">图片语义</option>
                </select>
              </div>
            </template>

            <template v-else-if="form.type === 'rustfs'">
              <div class="field"><label>Endpoint</label><input v-model="form.rustfs_endpoint" /></div>
              <div class="field"><label>Bucket</label><input v-model="form.rustfs_bucket" /></div>
              <div class="field"><label>Region</label><input v-model="form.rustfs_region" /></div>
              <div class="field"><label>Access Key</label><input v-model="form.rustfs_access_key" /></div>
              <div class="field"><label>Secret Key</label><input v-model="form.rustfs_secret_key" /></div>
              <div class="field"><label>Public Base URL</label><input v-model="form.rustfs_public_base_url" /></div>
              <div class="field-full field-check">
                <input id="rustfs-path-style" v-model="form.rustfs_path_style" type="checkbox" />
                <label for="rustfs-path-style">使用 path-style</label>
              </div>
            </template>

            <template v-else-if="isBotAdapterConnectionType(form.type)">
              <div class="field"><label>Bot WS URL</label><input v-model="form.bot_server_url" placeholder="ws://192.168.71.2:3008" /></div>
              <div class="field"><label>Adapter HTTP URL</label><input v-model="form.adapter_server_url" placeholder="http://192.168.71.2:3001" /></div>
              <div class="field"><label>QQ 号</label><input v-model="form.qq_id" /></div>
              <div class="field-full"><label>Token</label><input v-model="form.bot_server_token" /></div>
            </template>

            <template v-else-if="form.type === 'tavily'">
              <div class="field-full"><label>API Token（可选）</label><input v-model="form.tavily_api_token" type="password" placeholder="可选" /></div>
              <div class="field"><label>Timeout</label><input v-model.number="form.tavily_timeout_secs" type="number" min="1" /></div>
            </template>
          </div>
          <div class="panel-actions connection-picker-form-actions">
            <button class="btn ghost" @click="showEditor = false">返回</button>
            <button class="btn primary" @click="submitForm">创建连接</button>
          </div>
        </div>
        <div v-else class="connection-picker-grid">
          <button
            v-for="type in connectionTypes"
            :key="type.value"
            class="connection-picker-option"
            @click="pickCreateType(type.value)"
          >
            <strong>{{ type.label }}</strong>
            <span>{{ type.hint }}</span>
          </button>
        </div>
      </div>
    </div>

    <section v-if="connections.length > 0" class="panel">
      <div class="connection-grid" style="margin-top: 0;">
        <article
          v-for="connection in connections"
          :key="connection.config_id"
          :class="['connection-card', { 'connection-card--editing': form.id === connection.config_id }]"
        >
          <template v-if="form.id === connection.config_id">
            <div class="connection-card-header connection-card-header--stacked">
              <div class="connection-card-header-top">
                <div class="connection-card-badges">
                  <span class="badge">{{ form.type }}</span>
                  <span class="badge" :class="form.enabled ? 'success' : ''">{{ form.enabled ? "已启用" : "已停用" }}</span>
                </div>
                <div class="inline-actions connection-card-edit-actions">
                  <button class="btn primary connection-card-compact-btn" @click="submitForm">保存</button>
                  <button class="btn ghost connection-card-compact-btn" @click="closeEditor">取消</button>
                </div>
              </div>
              <div class="connection-card-title-edit">
                <input v-model="form.name" class="connection-card-inline-input connection-card-inline-input--title" />
              </div>
            </div>

            <div class="connection-card-body">
              <div class="key-value connection-card-edit-row">
                <strong>类型</strong>
                <select v-model="form.type" class="connection-card-inline-input">
                  <option value="mysql">MySQL</option>
                  <option value="redis">Redis</option>
                  <option value="weaviate">Weaviate</option>
                  <option value="rustfs">RustFS</option>
                  <option value="bot_adapter">Bot Adapter</option>
                  <option value="tavily">Tavily</option>
                </select>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>启用</strong>
                <label class="connection-card-inline-check">
                  <input :id="`connection-enabled-${connection.config_id}`" v-model="form.enabled" type="checkbox" />
                  <span>{{ form.enabled ? "已启用" : "已停用" }}</span>
                </label>
              </div>

              <template v-if="form.type === 'mysql'">
                <div class="key-value connection-card-edit-row">
                  <strong>地址</strong>
                  <input v-model="form.mysql_host" class="connection-card-inline-input" placeholder="127.0.0.1" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>端口</strong>
                  <input v-model="form.mysql_port" class="connection-card-inline-input" placeholder="3306" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>账号（可选）</strong>
                  <input v-model="form.mysql_user" class="connection-card-inline-input" placeholder="可选" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>密码（可选）</strong>
                  <input v-model="form.mysql_password" class="connection-card-inline-input" type="password" placeholder="请输入密码" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>数据库</strong>
                  <input v-model="form.mysql_database" class="connection-card-inline-input" placeholder="zihuan" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>最大连接数</strong>
                  <input v-model.number="form.mysql_max_connections" class="connection-card-inline-input" type="number" min="1" step="1" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>获取超时</strong>
                  <input v-model.number="form.mysql_acquire_timeout_secs" class="connection-card-inline-input" type="number" min="1" step="1" />
                </div>
              </template>

              <template v-else-if="form.type === 'redis'">
                <div class="key-value connection-card-edit-row">
                  <strong>URL</strong>
                  <input v-model="form.redis_url" class="connection-card-inline-input" placeholder="redis://127.0.0.1:6379" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>用户名（可选）</strong>
                  <input v-model="form.redis_username" class="connection-card-inline-input" placeholder="default" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>密码（可选）</strong>
                  <input v-model="form.redis_password" class="connection-card-inline-input" type="password" />
                </div>
              </template>

              <template v-else-if="form.type === 'weaviate'">
                <div class="key-value connection-card-edit-row">
                  <strong>Base URL</strong>
                  <input v-model="form.weaviate_base_url" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Class</strong>
                  <input v-model="form.weaviate_class_name" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>用户名（可选）</strong>
                  <input v-model="form.weaviate_username" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>密码（可选）</strong>
                  <input v-model="form.weaviate_password" class="connection-card-inline-input" type="password" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>API Key（可选）</strong>
                  <input v-model="form.weaviate_api_key" class="connection-card-inline-input" type="password" placeholder="可选" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Schema</strong>
                  <select v-model="form.weaviate_collection_schema" class="connection-card-inline-input">
                    <option value="message_record_semantic">消息记录语义</option>
                    <option value="image_semantic">图片语义</option>
                  </select>
                </div>
              </template>

              <template v-else-if="form.type === 'rustfs'">
                <div class="key-value connection-card-edit-row">
                  <strong>Endpoint</strong>
                  <input v-model="form.rustfs_endpoint" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Bucket</strong>
                  <input v-model="form.rustfs_bucket" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Region</strong>
                  <input v-model="form.rustfs_region" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Access Key</strong>
                  <input v-model="form.rustfs_access_key" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Secret Key</strong>
                  <input v-model="form.rustfs_secret_key" class="connection-card-inline-input" type="password" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Public URL</strong>
                  <input v-model="form.rustfs_public_base_url" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Path Style</strong>
                  <label class="connection-card-inline-check">
                    <input :id="`rustfs-path-style-${connection.config_id}`" v-model="form.rustfs_path_style" type="checkbox" />
                    <span>{{ form.rustfs_path_style ? "开启" : "关闭" }}</span>
                  </label>
                </div>
              </template>

              <template v-else-if="isBotAdapterConnectionType(form.type)">
                <div class="key-value connection-card-edit-row">
                  <strong>WS</strong>
                  <input v-model="form.bot_server_url" class="connection-card-inline-input" placeholder="ws://192.168.71.2:3008" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>HTTP</strong>
                  <input v-model="form.adapter_server_url" class="connection-card-inline-input" placeholder="http://192.168.71.2:3001" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>QQ</strong>
                  <input v-model="form.qq_id" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Token</strong>
                  <input v-model="form.bot_server_token" class="connection-card-inline-input" type="password" />
                </div>
              </template>

              <template v-else-if="form.type === 'tavily'">
                <div class="key-value connection-card-edit-row">
                  <strong>API Token（可选）</strong>
                  <input v-model="form.tavily_api_token" class="connection-card-inline-input" type="password" placeholder="可选" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Timeout</strong>
                  <input v-model.number="form.tavily_timeout_secs" class="connection-card-inline-input" type="number" min="1" />
                </div>
              </template>
            </div>
          </template>

          <template v-else>
            <div class="connection-card-header connection-card-header--stacked">
              <div class="connection-card-header-top">
                <div class="connection-card-badges">
                  <span class="badge">{{ connection.kind.type }}</span>
                  <span class="badge" :class="connection.enabled ? 'success' : ''">{{ connection.enabled ? "已启用" : "已停用" }}</span>
                </div>
                <div class="inline-actions connection-card-display-actions">
                  <button class="btn ghost connection-card-compact-btn" @click="editConnection(connection)">编辑</button>
                  <button class="btn warn connection-card-compact-btn" @click="removeConnection(connection.config_id)">删除</button>
                </div>
              </div>
              <h4>{{ connection.name }}</h4>
            </div>

            <div class="connection-card-body">
              <div v-for="item in summarizeConnection(connection)" :key="item.label" class="key-value">
                <strong>{{ item.label }}</strong>
                <span class="mono">{{ item.value }}</span>
              </div>
            </div>

            <div class="connection-card-footer">
              <span class="muted">更新于 {{ formatTime(connection.updated_at) }}</span>
            </div>
          </template>
        </article>
      </div>
    </section>
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";

import { ApiError, system, type ConnectionConfig, type RuntimeConnectionInstanceSummary } from "../../api/client";
import {
  buildConnectionPayload,
  compactId,
  connectionFormFromConfig,
  DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS,
  DEFAULT_MYSQL_MAX_CONNECTIONS,
  defaultConnectionForm,
  formatTime,
  isBotAdapterConnectionType,
  summarizeIds,
  type ConnectionFormState,
} from "../model";

type ConnectionTypeOption = {
  value: ConnectionFormState["type"];
  label: string;
  hint: string;
};

const connectionTypes: ConnectionTypeOption[] = [
  { value: "mysql", label: "MySQL", hint: "数据库连接" },
  { value: "redis", label: "Redis", hint: "缓存与会话" },
  { value: "weaviate", label: "Weaviate", hint: "向量检索" },
  { value: "rustfs", label: "RustFS", hint: "对象存储" },
  { value: "bot_adapter", label: "Bot Adapter", hint: "Bot 服务接入" },
  { value: "tavily", label: "Tavily", hint: "Tavily 搜索配置" },
];

const connections = ref<ConnectionConfig[]>([]);
const runtimeInstances = ref<RuntimeConnectionInstanceSummary[]>([]);
const form = reactive<ConnectionFormState>(defaultConnectionForm());
const showEditor = ref(false);
const showCreatePicker = ref(false);

function resetForm() {
  Object.assign(form, defaultConnectionForm());
}

function startCreate() {
  if (form.id) {
    resetForm();
    showEditor.value = false;
  } else if (showEditor.value) {
    showEditor.value = false;
    return;
  }
  showCreatePicker.value = true;
}

function closeCreatePicker() {
  resetForm();
  showEditor.value = false;
  showCreatePicker.value = false;
}

function pickCreateType(type: ConnectionFormState["type"]) {
  resetForm();
  form.type = type;
  showCreatePicker.value = true;
  showEditor.value = true;
}

function closeEditor() {
  resetForm();
  showEditor.value = false;
  showCreatePicker.value = false;
}

async function load() {
  const [loadedConnections, runtimeResponse] = await Promise.all([
    system.connections.list(),
    system.connections.listRuntimeInstances({ page: 1, page_size: 200 }),
  ]);
  connections.value = loadedConnections;
  runtimeInstances.value = runtimeResponse.items;
}

function editConnection(connection: ConnectionConfig) {
  Object.assign(form, connectionFormFromConfig(connection));
  showEditor.value = false;
}

async function submitForm() {
  if (!form.name.trim()) {
    alert("请填写连接名称");
    return;
  }
  if (form.type === "mysql") {
    if (!form.mysql_host.trim()) {
      alert("请填写 MySQL 地址");
      return;
    }
    if (!form.mysql_port.trim()) {
      alert("请填写 MySQL 端口");
      return;
    }
    if (!form.mysql_database.trim()) {
      alert("请填写 MySQL 数据库名");
      return;
    }
    if (!Number.isInteger(form.mysql_max_connections) || form.mysql_max_connections <= 0) {
      alert("请填写大于 0 的 MySQL 最大连接数");
      return;
    }
    if (
      !Number.isInteger(form.mysql_acquire_timeout_secs) ||
      form.mysql_acquire_timeout_secs <= 0
    ) {
      alert("请填写大于 0 的 MySQL 获取连接超时秒数");
      return;
    }
  }
  if (form.type === "weaviate") {
    if (!form.weaviate_base_url.trim()) {
      alert("请填写 Weaviate Base URL");
      return;
    }
    if (!form.weaviate_class_name.trim()) {
      alert("请填写 Weaviate Class Name");
      return;
    }
  }
  try {
    const payload = buildConnectionPayload(form);
    const result = await saveConnection(payload, false);
    if (!result) return;
    if (result.collection_created) {
      alert(`已自动创建 Weaviate collection: ${form.weaviate_class_name.trim()}`);
    }
    closeEditor();
    await load();
  } catch (error) {
    console.error("连接保存失败", error);
    alert(`连接保存失败：${formatSaveErrorMessage(error)}`);
  }
}

async function saveConnection(
  payload: ReturnType<typeof buildConnectionPayload>,
  allowCreateCollection: boolean,
) {
  try {
    const requestPayload = {
      ...payload,
      allow_create_collection: allowCreateCollection,
    };
    if (form.id) {
      return await system.connections.update(form.id, requestPayload);
    }
    return await system.connections.create(requestPayload);
  } catch (error) {
    if (error instanceof ApiError && error.code === "weaviate_collection_missing") {
      const className = String(error.details.class_name ?? form.weaviate_class_name.trim());
      if (window.confirm(`Weaviate collection "${className}" 不存在，是否自动新建？`)) {
        return await saveConnection(payload, true);
      }
      return null;
    }
    throw error;
  }
}

function formatSaveErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    const rawMessage = String(error.message || "").trim();
    const lowerMessage = rawMessage.toLowerCase();

    if (
      error.status === 401 ||
      lowerMessage.includes("anonymous access not enabled") ||
      lowerMessage.includes("authenticate through one of the available methods")
    ) {
      return "Weaviate 鉴权失败：请检查用户名/密码或 API Key 是否正确；如果实例允许匿名访问，也可以在 Weaviate 端开启匿名访问。";
    }

    if (rawMessage) {
      return rawMessage;
    }
    return `请求失败（HTTP ${error.status}）`;
  }

  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  return "未知错误，请查看浏览器控制台日志。";
}

async function removeConnection(id: string) {
  if (!window.confirm("确认删除这个连接配置吗？")) {
    return;
  }
  await system.connections.delete(id);
  if (form.id === id) {
    closeEditor();
  }
  await load();
}

function summarizeConnection(connection: ConnectionConfig): Array<{ label: string; value: string }> {
  const base = [
    { label: "Config ID", value: compactId(connection.config_id) },
    { label: "实例", value: summarizeConnectionInstances(connection.config_id) },
  ];
  const kind = connection.kind as Record<string, unknown>;
  switch (kind.type) {
    case "mysql":
      return [
        ...base,
        ...summarizeMysqlUrl(String(kind.url ?? "")),
        {
          label: "最大连接数",
          value: String(kind.max_connections ?? DEFAULT_MYSQL_MAX_CONNECTIONS),
        },
        {
          label: "获取超时",
          value: `${String(kind.acquire_timeout_secs ?? DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS)}s`,
        },
      ];
    case "redis":
      return [...base, { label: "URL", value: String(kind.url ?? "") }];
    case "weaviate":
      return [
        ...base,
        { label: "Base URL", value: String(kind.base_url ?? "") },
        { label: "Class", value: String(kind.class_name ?? "") },
        { label: "API Key", value: String(kind.api_key ?? "") ? "已配置" : "未设置" },
        { label: "Schema", value: formatWeaviateSchema(String(kind.collection_schema ?? "")) },
      ];
    case "rustfs":
      return [
        ...base,
        { label: "Endpoint", value: String(kind.endpoint ?? "") },
        { label: "Bucket", value: String(kind.bucket ?? "") },
      ];
    case "bot_adapter":
    case "ims_bot_adapter":
      return [
        ...base,
        { label: "WS", value: String(kind.bot_server_url ?? "") },
        { label: "HTTP", value: String(kind.adapter_server_url ?? "") || "未设置（默认由 WS 推导）" },
        { label: "QQ", value: String(kind.qq_id ?? "") || "未设置" },
      ];
    case "tavily":
      return [
        ...base,
        { label: "API Token", value: String(kind.api_token ?? "") ? "已配置" : "未设置" },
        { label: "Timeout", value: String(kind.timeout_secs ?? 30) },
      ];
    default:
      return base;
  }
}

function formatWeaviateSchema(schema: string): string {
  if (schema === "image_semantic") return "图片语义";
  if (schema === "message_record_semantic") return "消息记录语义";
  return schema || "未设置";
}

function summarizeConnectionInstances(configId: string): string {
  const ids = runtimeInstances.value
    .filter((item) => item.config_id === configId)
    .map((item) => item.instance_id);
  return summarizeIds(ids);
}

function summarizeMysqlUrl(rawUrl: string): Array<{ label: string; value: string }> {
  if (!rawUrl) {
    return [{ label: "URL", value: "" }];
  }
  try {
    const parsed = new URL(rawUrl);
    return [
      { label: "地址", value: decodeURIComponent(parsed.hostname ?? "") },
      { label: "端口", value: parsed.port || "3306" },
      { label: "账号", value: decodeURIComponent(parsed.username ?? "") || "未设置" },
      { label: "数据库", value: decodeURIComponent(parsed.pathname.replace(/^\//, "")) || "未设置" },
    ];
  } catch {
    return [{ label: "URL", value: rawUrl }];
  }
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`连接配置加载失败: ${(error as Error).message}`);
  });
});
</script>
