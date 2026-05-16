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
          <input v-model="redis.pattern" class="field-input" placeholder="* 或 openai_message_session:*" />
        </label>
        <div class="field" style="flex: 0; align-self: flex-end; display: flex; gap: 6px;">
          <button class="btn ghost" @click="redis.pattern = '*'">All Keys</button>
          <button class="btn ghost" @click="redis.pattern = 'openai_message_session:*'">LLM Sessions</button>
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
            <div v-if="isMessageWeaviateSchema && readStringProperty(item.properties, 'content')" class="explorer-weaviate-content">
              {{ readStringProperty(item.properties, 'content') }}
            </div>
            <div v-if="isImageWeaviateSchema && readStringProperty(item.properties, 'description')" class="explorer-weaviate-content">
              {{ readStringProperty(item.properties, 'description') }}
            </div>
            <div class="explorer-weaviate-grid">
              <template v-if="isMessageWeaviateSchema">
                <div v-if="readStringProperty(item.properties, 'sender_name')" class="key-value"><strong>Sender</strong><span>{{ readStringProperty(item.properties, 'sender_name') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'sender_id')" class="key-value"><strong>Sender ID</strong><span class="mono">{{ readStringProperty(item.properties, 'sender_id') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'group_name')" class="key-value"><strong>Group</strong><span>{{ readStringProperty(item.properties, 'group_name') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'group_id')" class="key-value"><strong>Group ID</strong><span class="mono">{{ readStringProperty(item.properties, 'group_id') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'send_time')" class="key-value"><strong>Send Time</strong><span>{{ readStringProperty(item.properties, 'send_time') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'message_id')" class="key-value"><strong>Message ID</strong><span class="mono">{{ readStringProperty(item.properties, 'message_id') }}</span></div>
              </template>
              <template v-else-if="isImageWeaviateSchema">
                <div v-if="readStringProperty(item.properties, 'name')" class="key-value"><strong>Name</strong><span>{{ readStringProperty(item.properties, 'name') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'media_id')" class="key-value"><strong>Media ID</strong><span class="mono">{{ readStringProperty(item.properties, 'media_id') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'source')" class="key-value"><strong>Source</strong><span>{{ readStringProperty(item.properties, 'source') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'mime_type')" class="key-value"><strong>MIME</strong><span>{{ readStringProperty(item.properties, 'mime_type') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'rustfs_path')" class="key-value"><strong>RustFS Path</strong><span class="mono">{{ readStringProperty(item.properties, 'rustfs_path') }}</span></div>
                <div v-if="readStringProperty(item.properties, 'original_source')" class="key-value"><strong>Original Source</strong><span class="mono">{{ readStringProperty(item.properties, 'original_source') }}</span></div>
              </template>
            </div>
            <details class="explorer-weaviate-details">
              <summary>查看原始字段</summary>
              <pre>{{ stringifyWeaviateProperties(item.properties) }}</pre>
            </details>
          </div>
        </article>
      </div>
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
import { ref, computed, onMounted } from "vue";
import {
  system,
  explorer,
  type ConnectionConfig,
  type LlmConfig,
  type MysqlRecord,
  type RedisKeyEntry,
  type RustfsObject,
  type WeaviateSearchResult,
} from "../../api/client";
import type { WeaviateCollectionSchema } from "../model";

const tabs = [
  { id: "mysql" as const, label: "MySQL" },
  { id: "redis" as const, label: "Redis" },
  { id: "weaviate" as const, label: "Weaviate" },
  { id: "rustfs" as const, label: "RustFS" },
];

const activeTab = ref<"mysql" | "redis" | "weaviate" | "rustfs">("mysql");
const connections = ref<ConnectionConfig[]>([]);
const llmRefs = ref<LlmConfig[]>([]);

const mysqlGoto = ref(1);
const redisGoto = ref(1);
const rustfsGoto = ref(1);

const mysqlConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "mysql" && c.enabled)
);
const redisConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "redis" && c.enabled)
);
const weaviateConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "weaviate" && c.enabled)
);
const selectedWeaviateConnection = computed(
  () => connections.value.find((c) => c.config_id === weaviate.value.connectionId) ?? null
);
const selectedWeaviateClassName = computed(() =>
  typeof selectedWeaviateConnection.value?.kind.class_name === "string"
    ? selectedWeaviateConnection.value.kind.class_name
    : weaviate.value.className
);
const selectedWeaviateSchema = computed(() => {
  const schema = selectedWeaviateConnection.value?.kind.collection_schema;
  return typeof schema === "string" ? schema as WeaviateCollectionSchema : weaviate.value.collectionSchema;
});
const isMessageWeaviateSchema = computed(() => selectedWeaviateSchema.value === "message_record_semantic");
const isImageWeaviateSchema = computed(() => selectedWeaviateSchema.value === "image_semantic");
const rustfsConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "rustfs" && c.enabled)
);
const embeddingModels = computed(() =>
  llmRefs.value.filter((item) => item.model.type === "text_embedding_local" && item.enabled)
);

function switchTab(tab: "mysql" | "redis" | "weaviate" | "rustfs") {
  activeTab.value = tab;
}

// ── MySQL ──────────────────────────────────────────────────────

const mysql = ref({
  connectionId: "",
  loading: false,
  searched: false,
  records: [] as MysqlRecord[],
  total: 0,
  page: 1,
  pageSize: 20,
  filters: {
    message_id: "",
    sender_id: "",
    sender_name: "",
    group_id: "",
    content: "",
    send_time_start: "",
    send_time_end: "",
  },
});

const mysqlTotalPages = computed(() =>
  Math.max(1, Math.ceil(mysql.value.total / mysql.value.pageSize))
);

function onMysqlConnectionChange() {
  mysql.value.records = [];
  mysql.value.total = 0;
  mysql.value.page = 1;
  mysql.value.searched = false;
}

async function searchMysql() {
  if (!mysql.value.connectionId) return;
  mysql.value.loading = true;
  mysql.value.searched = true;
  try {
    const f = mysql.value.filters;
    const res = await explorer.queryMysql({
      connection_id: mysql.value.connectionId,
      message_id: f.message_id || undefined,
      sender_id: f.sender_id || undefined,
      sender_name: f.sender_name || undefined,
      group_id: f.group_id || undefined,
      content: f.content || undefined,
      send_time_start: f.send_time_start || undefined,
      send_time_end: f.send_time_end || undefined,
      page: mysql.value.page,
      page_size: mysql.value.pageSize,
    });
    mysql.value.records = res.records;
    mysql.value.total = res.total;
  } catch (e: unknown) {
    alert((e as Error).message);
  } finally {
    mysql.value.loading = false;
  }
}

function mysqlPage(p: number) {
  mysql.value.page = Math.max(1, Math.min(p, mysqlTotalPages.value));
  mysqlGoto.value = mysql.value.page;
  searchMysql();
}

// ── Redis ──────────────────────────────────────────────────────

const redis = ref({
  connectionId: "",
  loading: false,
  searched: false,
  keys: [] as RedisKeyEntry[],
  total: 0,
  page: 1,
  pageSize: 20,
  pattern: "*",
  scanCursor: 0,
});

const redisTotalPages = computed(() =>
  Math.max(1, Math.ceil(redis.value.total / redis.value.pageSize))
);

function onRedisConnectionChange() {
  redis.value.keys = [];
  redis.value.total = 0;
  redis.value.page = 1;
  redis.value.scanCursor = 0;
  redis.value.searched = false;
}

async function searchRedis() {
  if (!redis.value.connectionId) return;
  redis.value.loading = true;
  redis.value.searched = true;
  try {
    const res = await explorer.queryRedis({
      connection_id: redis.value.connectionId,
      pattern: redis.value.pattern || undefined,
      page: redis.value.page,
      page_size: redis.value.pageSize,
      scan_cursor: redis.value.scanCursor || undefined,
    });
    redis.value.keys = res.keys;
    redis.value.total = res.total;
    redis.value.scanCursor = res.scan_cursor;
  } catch (e: unknown) {
    alert((e as Error).message);
  } finally {
    redis.value.loading = false;
  }
}

function redisPage(p: number) {
  redis.value.page = Math.max(1, Math.min(p, redisTotalPages.value));
  redisGoto.value = redis.value.page;
  searchRedis();
}

function formatTTL(ttl: number): string {
  if (ttl === -1) return "永不过期";
  if (ttl === -2) return "已过期";
  if (ttl < 60) return `${ttl}s`;
  if (ttl < 3600) return `${Math.floor(ttl / 60)}m ${ttl % 60}s`;
  const h = Math.floor(ttl / 3600);
  const m = Math.floor((ttl % 3600) / 60);
  return `${h}h ${m}m`;
}

// ── Weaviate ─────────────────────────────────────────────────

const weaviate = ref({
  connectionId: "",
  embeddingModelRefId: "",
  loading: false,
  searched: false,
  query: "",
  limit: 10,
  className: "",
  collectionSchema: "message_record_semantic" as WeaviateCollectionSchema,
  items: [] as WeaviateSearchResult[],
});

function onWeaviateConnectionChange() {
  weaviate.value.items = [];
  weaviate.value.className = "";
  weaviate.value.collectionSchema = "message_record_semantic";
  weaviate.value.searched = false;
}

function onWeaviateEmbeddingChange() {
  weaviate.value.items = [];
  weaviate.value.searched = false;
}

async function searchWeaviate() {
  if (!weaviate.value.connectionId) {
    return;
  }
  if (!weaviate.value.embeddingModelRefId) {
    alert("请选择 Text Embedding 模型");
    return;
  }
  weaviate.value.loading = true;
  weaviate.value.searched = true;
  try {
    const res = await explorer.queryWeaviate({
      connection_id: weaviate.value.connectionId,
      embedding_model_ref_id: weaviate.value.embeddingModelRefId,
      query: weaviate.value.query.trim(),
      limit: weaviate.value.limit,
    });
    weaviate.value.items = res.items;
    weaviate.value.className = res.class_name;
    weaviate.value.collectionSchema = res.collection_schema;
  } catch (e: unknown) {
    alert((e as Error).message);
  } finally {
    weaviate.value.loading = false;
  }
}

function readStringProperty(properties: Record<string, unknown>, key: string): string {
  const value = properties[key];
  return typeof value === "string" ? value : "";
}

function formatWeaviateDistance(distance: number | null): string {
  return typeof distance === "number" ? distance.toFixed(4) : "—";
}

function stringifyWeaviateProperties(properties: Record<string, unknown>): string {
  return JSON.stringify(properties, null, 2);
}

// ── RustFS ─────────────────────────────────────────────────────

const rustfs = ref({
  connectionId: "",
  loading: false,
  searched: false,
  objects: [] as RustfsObject[],
  prefixes: [] as string[],
  total: 0,
  page: 1,
  pageSize: 20,
  prefix: "",
  search: "",
});

const rustfsTotalPages = computed(() =>
  Math.max(1, Math.ceil(rustfs.value.total / rustfs.value.pageSize))
);

function onRustfsConnectionChange() {
  rustfs.value.objects = [];
  rustfs.value.prefixes = [];
  rustfs.value.total = 0;
  rustfs.value.page = 1;
  rustfs.value.prefix = "";
  rustfs.value.searched = false;
}

async function searchRustfs() {
  if (!rustfs.value.connectionId) return;
  rustfs.value.loading = true;
  rustfs.value.searched = true;
  try {
    const res = await explorer.queryRustfs({
      connection_id: rustfs.value.connectionId,
      prefix: rustfs.value.prefix || undefined,
      search: rustfs.value.search || undefined,
      page: rustfs.value.page,
      page_size: rustfs.value.pageSize,
    });
    rustfs.value.objects = res.objects;
    rustfs.value.prefixes = res.prefixes;
    rustfs.value.total = res.total;
  } catch (e: unknown) {
    alert((e as Error).message);
  } finally {
    rustfs.value.loading = false;
  }
}

function rustfsPage(p: number) {
  rustfs.value.page = Math.max(1, Math.min(p, rustfsTotalPages.value));
  rustfsGoto.value = rustfs.value.page;
  searchRustfs();
}

function navigateRustfsPrefix(segmentIndex: number) {
  const parts = rustfs.value.prefix.split("/").filter(Boolean);
  rustfs.value.prefix = parts.slice(0, segmentIndex + 1).join("/") + "/";
  searchRustfs();
}

function shortKey(key: string): string {
  const idx = key.lastIndexOf("/");
  return idx >= 0 ? key.slice(idx + 1) : key;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function onImageError(e: Event) {
  const img = e.target as HTMLImageElement;
  img.style.display = "none";
}

// ── Init ───────────────────────────────────────────────────────

onMounted(async () => {
  const [connectionResult, llmResult] = await Promise.allSettled([
    system.connections.list(),
    system.llm.list(),
  ]);
  if (connectionResult.status === "fulfilled") {
    connections.value = connectionResult.value;
  }
  if (llmResult.status === "fulfilled") {
    llmRefs.value = llmResult.value;
  }
});
</script>
