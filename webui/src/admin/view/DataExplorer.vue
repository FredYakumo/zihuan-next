<template>
  <section class="page data-explorer-page">
    <div class="page-hero">
      <h2>数据检索</h2>
    </div>

    <div class="explorer-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        :class="['explorer-tab', { 'explorer-tab--active': activeTab === tab.id }]"
        @click="switchTab(tab.id)"
      >{{ tab.label }}</button>
    </div>

    <!-- MySQL Tab -->
    <section v-if="activeTab === 'mysql'" class="panel">
      <div class="explorer-connection-select">
        <label class="field">
          <span class="field-label">MySQL 连接</span>
          <select v-model="mysql.connectionId" class="field-input" @change="onMysqlConnectionChange">
            <option value="">— 选择连接 —</option>
            <option v-for="c in mysqlConnections" :key="c.config_id" :value="c.config_id">{{ c.name }}</option>
          </select>
        </label>
      </div>

      <div v-if="mysql.connectionId" class="explorer-search">
        <label class="field">
          <span class="field-label">Message ID</span>
          <input v-model="mysql.filters.message_id" class="field-input" />
        </label>
        <label class="field">
          <span class="field-label">Sender ID</span>
          <input v-model="mysql.filters.sender_id" class="field-input" />
        </label>
        <label class="field">
          <span class="field-label">Sender Name</span>
          <input v-model="mysql.filters.sender_name" class="field-input" />
        </label>
        <label class="field">
          <span class="field-label">Group ID</span>
          <input v-model="mysql.filters.group_id" class="field-input" />
        </label>
        <label class="field">
          <span class="field-label">Content</span>
          <input v-model="mysql.filters.content" class="field-input" />
        </label>
        <label class="field">
          <span class="field-label">Send Time Start</span>
          <input v-model="mysql.filters.send_time_start" type="datetime-local" class="field-input" />
        </label>
        <label class="field">
          <span class="field-label">Send Time End</span>
          <input v-model="mysql.filters.send_time_end" type="datetime-local" class="field-input" />
        </label>
        <div class="field" style="align-self: flex-end;">
          <button class="btn" :disabled="mysql.loading" @click="searchMysql">搜索</button>
        </div>
      </div>

      <div v-if="mysql.loading" class="empty-state">加载中…</div>
      <div v-else-if="mysql.connectionId && mysql.records.length === 0 && mysql.searched" class="empty-state">无匹配记录。</div>
      <table v-else-if="mysql.records.length > 0" class="explorer-table">
        <thead>
          <tr>
            <th>Message ID</th>
            <th>Sender</th>
            <th>Send Time</th>
            <th>Group</th>
            <th>Content</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(r, i) in mysql.records" :key="i">
            <td class="td-mono">{{ r.message_id }}</td>
            <td>
              <div>{{ r.sender_name }}</div>
              <div class="muted" style="font-size:11px;">{{ r.sender_id }}</div>
            </td>
            <td>{{ r.send_time }}</td>
            <td>
              <template v-if="r.group_name">{{ r.group_name }}</template>
              <template v-else-if="r.group_id">{{ r.group_id }}</template>
              <span v-else class="muted">—</span>
            </td>
            <td class="td-content">{{ r.content }}</td>
          </tr>
        </tbody>
      </table>

      <div v-if="mysql.records.length > 0" class="explorer-pagination">
        <select v-model="mysql.pageSize" class="field-input" style="width:80px" @change="mysqlPage(1)">
          <option :value="20">20/页</option>
          <option :value="50">50/页</option>
          <option :value="100">100/页</option>
        </select>
        <div class="explorer-pagination-divider"></div>
        <button class="btn ghost" :disabled="mysql.page <= 1" @click="mysqlPage(1)">首页</button>
        <button class="btn ghost" :disabled="mysql.page <= 1" @click="mysqlPage(mysql.page - 1)">上一页</button>
        <span class="explorer-pagination-info">{{ mysql.page }} / {{ mysqlTotalPages }} ({{ mysql.total }} 条)</span>
        <input type="number" v-model.number="mysqlGoto" class="field-input" style="width:56px;text-align:center" min="1" :max="mysqlTotalPages" @keyup.enter="mysqlPage(mysqlGoto)" />
        <button class="btn ghost" @click="mysqlPage(mysqlGoto)">跳转</button>
        <button class="btn ghost" :disabled="mysql.page >= mysqlTotalPages" @click="mysqlPage(mysql.page + 1)">下一页</button>
        <button class="btn ghost" :disabled="mysql.page >= mysqlTotalPages" @click="mysqlPage(mysqlTotalPages)">末页</button>
      </div>
    </section>

    <!-- Redis Tab -->
    <section v-if="activeTab === 'redis'" class="panel">
      <div class="explorer-connection-select">
        <label class="field">
          <span class="field-label">Redis 连接</span>
          <select v-model="redis.connectionId" class="field-input" @change="onRedisConnectionChange">
            <option value="">— 选择连接 —</option>
            <option v-for="c in redisConnections" :key="c.config_id" :value="c.config_id">{{ c.name }}</option>
          </select>
        </label>
      </div>

      <div v-if="redis.connectionId" class="explorer-search">
        <label class="field" style="flex: 2;">
          <span class="field-label">Key Pattern</span>
          <input v-model="redis.pattern" class="field-input" placeholder="* 或 llm_message_session:*" />
        </label>
        <div class="field" style="flex: 0; align-self: flex-end; display: flex; gap: 6px;">
          <button class="btn ghost" @click="redis.pattern = '*'">All Keys</button>
          <button class="btn ghost" @click="redis.pattern = 'llm_message_session:*'">LLM Sessions</button>
          <button class="btn" :disabled="redis.loading" @click="searchRedis">搜索</button>
        </div>
      </div>

      <div v-if="redis.loading" class="empty-state">扫描中…</div>
      <div v-else-if="redis.connectionId && redis.keys.length === 0 && redis.searched" class="empty-state">无匹配键。</div>
      <div v-else-if="redis.keys.length > 0" class="explorer-redis-list">
        <article v-for="(entry, i) in redis.keys" :key="i" class="explorer-redis-key">
          <div class="explorer-redis-key-header">
            <span class="td-mono">{{ entry.key }}</span>
            <span class="badge">{{ entry.key_type }}</span>
            <span class="muted" style="font-size:11px;">TTL: {{ formatTTL(entry.ttl) }}</span>
          </div>
          <pre v-if="entry.value_preview" class="explorer-redis-value">{{ entry.value_preview }}</pre>
        </article>
      </div>

      <div v-if="redis.keys.length > 0" class="explorer-pagination">
        <select v-model="redis.pageSize" class="field-input" style="width:80px" @change="redisPage(1)">
          <option :value="20">20/页</option>
          <option :value="50">50/页</option>
          <option :value="100">100/页</option>
        </select>
        <div class="explorer-pagination-divider"></div>
        <button class="btn ghost" :disabled="redis.page <= 1" @click="redisPage(1)">首页</button>
        <button class="btn ghost" :disabled="redis.page <= 1" @click="redisPage(redis.page - 1)">上一页</button>
        <span class="explorer-pagination-info">{{ redis.page }} / {{ redisTotalPages }} ({{ redis.total }} 键)</span>
        <input type="number" v-model.number="redisGoto" class="field-input" style="width:56px;text-align:center" min="1" :max="redisTotalPages" @keyup.enter="redisPage(redisGoto)" />
        <button class="btn ghost" @click="redisPage(redisGoto)">跳转</button>
        <button class="btn ghost" :disabled="redis.page >= redisTotalPages" @click="redisPage(redis.page + 1)">下一页</button>
        <button class="btn ghost" :disabled="redis.page >= redisTotalPages" @click="redisPage(redisTotalPages)">末页</button>
      </div>
    </section>

    <!-- Weaviate Tab -->
    <section v-if="activeTab === 'weaviate'" class="panel">
      <div class="explorer-connection-select">
        <label class="field">
          <span class="field-label">Weaviate 连接</span>
          <select v-model="weaviate.connectionId" class="field-input" @change="onWeaviateConnectionChange">
            <option value="">— 选择连接 —</option>
            <option v-for="c in weaviateConnections" :key="c.config_id" :value="c.config_id">{{ c.name }}</option>
          </select>
        </label>
      </div>

      <div v-if="weaviate.connectionId" class="explorer-search">
        <label class="field">
          <span class="field-label">Text Embedding 模型</span>
          <select v-model="weaviate.embeddingModelRefId" class="field-input" @change="onWeaviateEmbeddingChange">
            <option value="">— 选择模型 —</option>
            <option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
          </select>
        </label>
        <label class="field" style="grid-column: span 2;">
          <span class="field-label">Query</span>
          <input v-model="weaviate.query" class="field-input" placeholder="输入要向量检索的文本" @keyup.enter="searchWeaviate" />
        </label>
        <label class="field">
          <span class="field-label">Limit</span>
          <input v-model.number="weaviate.limit" type="number" min="1" max="50" class="field-input" />
        </label>
        <div class="field" style="align-self: flex-end;">
          <button class="btn" :disabled="weaviate.loading" @click="searchWeaviate">搜索</button>
        </div>
      </div>

      <div v-if="selectedWeaviateConnection" class="explorer-meta muted">
        Class: {{ selectedWeaviateClassName }}
        <template v-if="selectedWeaviateSchema">
          · Schema: {{ selectedWeaviateSchema }}
        </template>
        <template v-if="isAgentMemorySchema">
          · <button class="btn ghost" style="margin-left: 8px;" @click="createAgentMemory">新建记忆</button>
        </template>
      </div>

      <div v-if="weaviate.loading" class="empty-state">检索中…</div>
      <div v-else-if="weaviate.connectionId && weaviate.items.length === 0 && weaviate.searched" class="empty-state">无匹配结果。</div>
      <div v-else-if="weaviate.items.length > 0" class="explorer-weaviate-list">
        <article v-for="(item, i) in weaviate.items" :key="i" class="explorer-weaviate-card">
          <div class="explorer-weaviate-card-header">
            <span class="badge">#{{ i + 1 }}</span>
            <span class="badge">distance {{ formatWeaviateDistance(item.distance) }}</span>
            <span v-if="item.object_id" class="td-mono explorer-weaviate-object-id">{{ item.object_id }}</span>
          </div>
          <div class="explorer-weaviate-card-body">
            <div v-if="isImageWeaviateSchema && readStringProperty(item.properties, 'description')" class="explorer-weaviate-content">
              {{ readStringProperty(item.properties, 'description') }}
            </div>
            <div v-if="isAgentMemorySchema && readStringProperty(item.properties, 'value')" class="explorer-weaviate-content">
              {{ readStringProperty(item.properties, 'value') }}
            </div>
            <div class="explorer-weaviate-grid">
              <template v-if="isImageWeaviateSchema">
                <div v-if="readStringProperty(item.properties, 'name')" class="key-value"><strong>Name</strong><span>{{ readStringProperty(item.properties, 'name') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'media_id')" class="key-value"><strong>Media ID</strong><span class="mono">{{ readStringProperty(item.properties, 'media_id') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'source')" class="key-value"><strong>Source</strong><span>{{ readStringProperty(item.properties, 'source') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'mime_type')" class="key-value"><strong>MIME</strong><span>{{ readStringProperty(item.properties, 'mime_type') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'rustfs_path')" class="key-value"><strong>RustFS Path</strong><span class="mono">{{ readStringProperty(item.properties, 'rustfs_path') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'original_source')" class="key-value"><strong>Original Source</strong><span class="mono">{{ readStringProperty(item.properties, 'original_source') }}</span></div>
              </template>
              <template v-else-if="isAgentMemorySchema">
                <div v-if="readStringProperty(item.properties, 'title')" class="key-value"><strong>标题</strong><span>{{ readStringProperty(item.properties, 'title') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'expires_at')" class="key-value"><strong>Expires</strong><span>{{ readStringProperty(item.properties, 'expires_at') }}</span></div>
                <div class="key-value"><strong>Sender Scope</strong><span>{{ readStringListProperty(item.properties, 'sender_id_list').join(", ") || "全局" }}</span></div>
                <div class="key-value"><strong>Group Scope</strong><span>{{ readStringListProperty(item.properties, 'group_id_list').join(", ") || "无" }}</span></div>
                <div v-if="readStringProperty(item.properties, 'created_at')" class="key-value"><strong>Created</strong><span>{{ readStringProperty(item.properties, 'created_at') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'updated_at')" class="key-value"><strong>Updated</strong><span>{{ readStringProperty(item.properties, 'updated_at') }}</span></div>
              </template>
            </div>
            <div v-if="isAgentMemorySchema && item.object_id" style="display: flex; gap: 8px; margin-top: 12px;">
              <button class="btn ghost" @click="editAgentMemory(item)">编辑</button>
              <button class="btn ghost" @click="removeAgentMemory(item)">删除</button>
            </div>
            <details class="explorer-weaviate-details">
              <summary>查看原始字段</summary>
              <pre>{{ stringifyWeaviateProperties(item.properties) }}</pre>
            </details>
          </div>
        </article>
      </div>
    </section>

    <!-- QQ Chat Rate Limit Usage Tab -->
    <section v-if="activeTab === 'qq_chat_rate_limit'" class="panel">
      <div class="explorer-connection-select">
        <label class="field">
          <span class="field-label">QQ Chat Agent Service</span>
          <select v-model="qqChatRateLimit.agentId" class="field-input" @change="onQqChatRateLimitAgentChange">
            <option value="">— 选择 Service —</option>
            <option v-for="service in qqChatServices" :key="service.config_id" :value="service.config_id">
              {{ service.name }}
            </option>
          </select>
        </label>
      </div>

      <div v-if="qqChatRateLimit.agentId" class="explorer-search">
        <div class="field" style="align-self: flex-end;">
          <button class="btn" :disabled="qqChatRateLimit.loading" @click="loadQqChatRateLimitUsage">刷新</button>
        </div>
      </div>

      <div v-if="qqChatRateLimit.loading" class="empty-state">加载中…</div>
      <div v-else-if="qqChatRateLimit.agentId && qqChatRateLimit.items.length === 0 && qqChatRateLimit.searched" class="empty-state">
        当前没有使用记录。
      </div>
      <table v-else-if="qqChatRateLimit.items.length > 0" class="explorer-table">
        <thead>
          <tr>
            <th>用户</th>
            <th>群组</th>
            <th>规则来源</th>
            <th>窗口</th>
            <th>用量</th>
            <th>更新时间</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="item in qqChatRateLimit.items" :key="`${item.sender_id}-${item.scope_type}-${item.scope_key}-${item.group_id || 'private'}`">
            <td>
              <div>{{ item.sender_name || "未知用户" }}</div>
              <div class="muted" style="font-size:11px;">{{ item.sender_id }}</div>
            </td>
            <td>
              <template v-if="item.group_name">{{ item.group_name }}</template>
              <template v-else-if="item.group_id">{{ item.group_id }}</template>
              <span v-else class="muted">—</span>
            </td>
            <td>{{ formatRateLimitScope(item.scope_type, item.scope_key) }}</td>
            <td>{{ formatRateLimitWindow(item.window_unit) }}</td>
            <td>{{ formatRateLimitUsage(item.used_calls, item.max_calls, item.unlimited) }}</td>
            <td>{{ item.updated_at }}</td>
            <td>
              <button
                class="btn ghost"
                :disabled="qqChatRateLimit.resettingSenderId === item.sender_id"
                @click="resetQqChatRateLimitUsage(item.sender_id)"
              >
                {{ qqChatRateLimit.resettingSenderId === item.sender_id ? "重置中…" : "清空当前计数" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </section>

    <!-- RustFS Tab -->
    <section v-if="activeTab === 'rustfs'" class="panel">
      <div class="explorer-connection-select">
        <label class="field">
          <span class="field-label">RustFS 连接</span>
          <select v-model="rustfs.connectionId" class="field-input" @change="onRustfsConnectionChange">
            <option value="">— 选择连接 —</option>
            <option v-for="c in rustfsConnections" :key="c.config_id" :value="c.config_id">{{ c.name }}</option>
          </select>
        </label>
      </div>

      <div v-if="rustfs.connectionId" class="explorer-search">
        <label class="field">
          <span class="field-label">Prefix</span>
          <input v-model="rustfs.prefix" class="field-input" placeholder="qq-images/" />
        </label>
        <label class="field">
          <span class="field-label">Search</span>
          <input v-model="rustfs.search" class="field-input" placeholder="文件名搜索" />
        </label>
        <div class="field" style="align-self: flex-end;">
          <button class="btn" :disabled="rustfs.loading" @click="searchRustfs">搜索</button>
        </div>
      </div>

      <div v-if="rustfs.prefix" class="explorer-breadcrumb">
        <span class="explorer-breadcrumb-segment" @click="rustfs.prefix = ''; searchRustfs()">Root</span>
        <template v-for="(seg, i) in rustfs.prefix.split('/').filter(Boolean)" :key="i">
          <span class="explorer-breadcrumb-separator">/</span>
          <span class="explorer-breadcrumb-segment" @click="navigateRustfsPrefix(i)">{{ seg }}</span>
        </template>
      </div>

      <div v-if="rustfs.prefixes.length > 0" class="explorer-folder-list">
        <button
          v-for="p in rustfs.prefixes"
          :key="p"
          class="explorer-folder-btn"
          @click="rustfs.prefix = p; searchRustfs()"
        >📁 {{ p }}</button>
      </div>

      <div v-if="rustfs.loading" class="empty-state">加载中…</div>
      <div v-else-if="rustfs.connectionId && rustfs.objects.length === 0 && rustfs.searched" class="empty-state">无匹配对象。</div>
      <div v-else-if="rustfs.objects.length > 0" class="explorer-image-grid">
        <div v-for="obj in rustfs.objects" :key="obj.key" class="explorer-image-card">
          <a :href="obj.url" target="_blank" rel="noopener">
            <img :src="obj.url" :alt="obj.key" loading="lazy" @error="onImageError" />
          </a>
          <div class="explorer-image-card-info">
            <div class="td-mono" style="font-size:12px;word-break:break-all;">{{ shortKey(obj.key) }}</div>
            <div class="muted" style="font-size:11px;">{{ formatSize(obj.size) }}</div>
            <div v-if="obj.last_modified" class="muted" style="font-size:11px;">{{ obj.last_modified }}</div>
          </div>
        </div>
      </div>

      <div v-if="rustfs.objects.length > 0" class="explorer-pagination">
        <select v-model="rustfs.pageSize" class="field-input" style="width:80px" @change="rustfsPage(1)">
          <option :value="20">20/页</option>
          <option :value="50">50/页</option>
          <option :value="100">100/页</option>
        </select>
        <div class="explorer-pagination-divider"></div>
        <button class="btn ghost" :disabled="rustfs.page <= 1" @click="rustfsPage(1)">首页</button>
        <button class="btn ghost" :disabled="rustfs.page <= 1" @click="rustfsPage(rustfs.page - 1)">上一页</button>
        <span class="explorer-pagination-info">{{ rustfs.page }} / {{ rustfsTotalPages }} ({{ rustfs.total }} 对象)</span>
        <input type="number" v-model.number="rustfsGoto" class="field-input" style="width:56px;text-align:center" min="1" :max="rustfsTotalPages" @keyup.enter="rustfsPage(rustfsGoto)" />
        <button class="btn ghost" @click="rustfsPage(rustfsGoto)">跳转</button>
        <button class="btn ghost" :disabled="rustfs.page >= rustfsTotalPages" @click="rustfsPage(rustfs.page + 1)">下一页</button>
        <button class="btn ghost" :disabled="rustfs.page >= rustfsTotalPages" @click="rustfsPage(rustfsTotalPages)">末页</button>
      </div>
    </section>
  </section>
</template>

<script setup lang="ts">
import { useDataExplorer } from "../composables/useDataExplorer";

const {
  tabs,
  activeTab,
  switchTab,
  mysqlConnections,
  mysql,
  mysqlGoto,
  mysqlTotalPages,
  onMysqlConnectionChange,
  searchMysql,
  mysqlPage,
  redisConnections,
  redis,
  redisGoto,
  redisTotalPages,
  onRedisConnectionChange,
  searchRedis,
  redisPage,
  formatTTL,
  weaviateConnections,
  weaviate,
  embeddingModels,
  selectedWeaviateConnection,
  selectedWeaviateClassName,
  selectedWeaviateSchema,
  isImageWeaviateSchema,
  isAgentMemorySchema,
  onWeaviateConnectionChange,
  onWeaviateEmbeddingChange,
  searchWeaviate,
  readStringProperty,
  readStringListProperty,
  formatWeaviateDistance,
  stringifyWeaviateProperties,
  createAgentMemory,
  editAgentMemory,
  removeAgentMemory,
  qqChatServices,
  qqChatRateLimit,
  onQqChatRateLimitAgentChange,
  loadQqChatRateLimitUsage,
  resetQqChatRateLimitUsage,
  formatRateLimitScope,
  formatRateLimitWindow,
  formatRateLimitUsage,
  rustfsConnections,
  rustfs,
  rustfsGoto,
  rustfsTotalPages,
  onRustfsConnectionChange,
  searchRustfs,
  rustfsPage,
  navigateRustfsPrefix,
  shortKey,
  formatSize,
  onImageError,
} = useDataExplorer();
</script>

<style scoped lang="scss">
@use "../styles/data-explorer" as *;
</style>
