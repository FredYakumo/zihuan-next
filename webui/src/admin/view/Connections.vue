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
                <label>账号</label>
                <input v-model="form.mysql_user" placeholder="root" />
              </div>
              <div class="field">
                <label>密码</label>
                <input v-model="form.mysql_password" type="password" placeholder="请输入密码" />
              </div>
              <div class="field-full">
                <label>数据库名</label>
                <input v-model="form.mysql_database" placeholder="zihuan" />
              </div>
            </template>

            <template v-else-if="form.type === 'redis'">
              <div class="field-full">
                <label>URL</label>
                <input v-model="form.redis_url" placeholder="redis://127.0.0.1:6379" />
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
              <div class="field-full"><label>API Token</label><input v-model="form.tavily_api_token" type="password" /></div>
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
          :key="connection.id"
          :class="['connection-card', { 'connection-card--editing': form.id === connection.id }]"
        >
          <template v-if="form.id === connection.id">
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
                  <input :id="`connection-enabled-${connection.id}`" v-model="form.enabled" type="checkbox" />
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
                  <strong>账号</strong>
                  <input v-model="form.mysql_user" class="connection-card-inline-input" placeholder="root" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>密码</strong>
                  <input v-model="form.mysql_password" class="connection-card-inline-input" type="password" placeholder="请输入密码" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>数据库</strong>
                  <input v-model="form.mysql_database" class="connection-card-inline-input" placeholder="zihuan" />
                </div>
              </template>

              <template v-else-if="form.type === 'redis'">
                <div class="key-value connection-card-edit-row">
                  <strong>URL</strong>
                  <input v-model="form.redis_url" class="connection-card-inline-input" placeholder="redis://127.0.0.1:6379" />
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
                    <input :id="`rustfs-path-style-${connection.id}`" v-model="form.rustfs_path_style" type="checkbox" />
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
                  <strong>API Token</strong>
                  <input v-model="form.tavily_api_token" class="connection-card-inline-input" type="password" />
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
                  <button class="btn warn connection-card-compact-btn" @click="removeConnection(connection.id)">删除</button>
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

import { system, type ConnectionConfig } from "../../api/client";
import {
  buildConnectionPayload,
  connectionFormFromConfig,
  defaultConnectionForm,
  formatTime,
  isBotAdapterConnectionType,
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
  connections.value = await system.connections.list();
}

function editConnection(connection: ConnectionConfig) {
  Object.assign(form, connectionFormFromConfig(connection));
  showEditor.value = false;
}

async function submitForm() {
  const payload = buildConnectionPayload(form);
  if (!payload.name) {
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
    if (!form.mysql_user.trim()) {
      alert("请填写 MySQL 账号");
      return;
    }
    if (!form.mysql_database.trim()) {
      alert("请填写 MySQL 数据库名");
      return;
    }
  }
  if (form.type === "tavily" && !form.tavily_api_token.trim()) {
    alert("请填写 Tavily API Token");
    return;
  }
  if (form.id) {
    await system.connections.update(form.id, payload);
  } else {
    await system.connections.create(payload);
  }
  closeEditor();
  await load();
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
  const kind = connection.kind as Record<string, unknown>;
  switch (kind.type) {
    case "mysql":
      return summarizeMysqlUrl(String(kind.url ?? ""));
    case "redis":
      return [{ label: "URL", value: String(kind.url ?? "") }];
    case "weaviate":
      return [
        { label: "Base URL", value: String(kind.base_url ?? "") },
        { label: "Class", value: String(kind.class_name ?? "") },
      ];
    case "rustfs":
      return [
        { label: "Endpoint", value: String(kind.endpoint ?? "") },
        { label: "Bucket", value: String(kind.bucket ?? "") },
      ];
    case "bot_adapter":
    case "ims_bot_adapter":
      return [
        { label: "WS", value: String(kind.bot_server_url ?? "") },
        { label: "HTTP", value: String(kind.adapter_server_url ?? "") || "未设置（默认由 WS 推导）" },
        { label: "QQ", value: String(kind.qq_id ?? "") || "未设置" },
      ];
    case "tavily":
      return [
        { label: "API Token", value: String(kind.api_token ?? "") ? "已配置" : "未设置" },
        { label: "Timeout", value: String(kind.timeout_secs ?? 30) },
      ];
    default:
      return [];
  }
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
