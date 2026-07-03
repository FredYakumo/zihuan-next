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
                <option value="web_search_engine">Web Search Engine</option>
                <option value="tokenizer">Tokenizer</option>
                <option value="sqlite">SQLite</option>
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
                  <option value="image_semantic">图片语义</option>
                  <option value="agent_memory">Agent 记忆</option>
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

            <template v-else-if="form.type === 'web_search_engine'">
              <div class="field">
                <label>Provider</label>
                <select v-model="form.web_search_engine_provider">
                  <option value="tavily">Tavily</option>
                  <option value="brave">Brave</option>
                </select>
              </div>
              <div class="field-full"><label>API Token（可选）</label><input v-model="form.web_search_engine_api_token" type="password" placeholder="可选" /></div>
              <div class="field"><label>Timeout</label><input v-model.number="form.web_search_engine_timeout_secs" type="number" min="1" /></div>
            </template>

            <template v-else-if="form.type === 'tokenizer'">
              <div class="field-full">
                <label>Tokenizer 模型</label>
                <select v-model="form.tokenizer_model_name">
                  <option value="">请选择</option>
                  <option v-for="model in tokenizerModels" :key="model" :value="model">{{ model }}</option>
                </select>
              </div>
            </template>

            <template v-else-if="form.type === 'sqlite'">
              <div class="field-full">
                <label>数据库文件路径</label>
                <input v-model="form.sqlite_path" placeholder="/path/to/database.db" />
              </div>
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
                  <option value="web_search_engine">Web Search Engine</option>
                  <option value="tokenizer">Tokenizer</option>
                  <option value="sqlite">SQLite</option>
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
                    <option value="image_semantic">图片语义</option>
                    <option value="agent_memory">Agent 记忆</option>
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

              <template v-else-if="form.type === 'web_search_engine'">
                <div class="key-value connection-card-edit-row">
                  <strong>Provider</strong>
                  <select v-model="form.web_search_engine_provider" class="connection-card-inline-input">
                    <option value="tavily">Tavily</option>
                    <option value="brave">Brave</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>API Token（可选）</strong>
                  <input v-model="form.web_search_engine_api_token" class="connection-card-inline-input" type="password" placeholder="可选" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Timeout</strong>
                  <input v-model.number="form.web_search_engine_timeout_secs" class="connection-card-inline-input" type="number" min="1" />
                </div>
              </template>

              <template v-else-if="form.type === 'tokenizer'">
                <div class="key-value connection-card-edit-row">
                  <strong>Tokenizer 模型</strong>
                  <select v-model="form.tokenizer_model_name" class="connection-card-inline-input">
                    <option value="">请选择</option>
                    <option v-for="model in tokenizerModels" :key="model" :value="model">{{ model }}</option>
                  </select>
                </div>
              </template>

              <template v-else-if="form.type === 'sqlite'">
                <div class="key-value connection-card-edit-row">
                  <strong>文件路径</strong>
                  <input v-model="form.sqlite_path" class="connection-card-inline-input" placeholder="/path/to/database.db" />
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
import { useConnections } from "../composables/useConnections";

const {
  connections,
  tokenizerModels,
  form,
  showEditor,
  showCreatePicker,
  connectionTypes,
  startCreate,
  closeCreatePicker,
  pickCreateType,
  closeEditor,
  editConnection,
  submitForm,
  removeConnection,
  summarizeConnection,
  formatTime,
  isBotAdapterConnectionType,
} = useConnections();
</script>

<style scoped lang="scss">
@use "../styles/connections" as *;
</style>
