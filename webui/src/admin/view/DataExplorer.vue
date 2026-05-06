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
            <option v-for="c in mysqlConnections" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
        </label>
      </div>

      <div v-if="mysql.connectionId" class="explorer-search">
        <label class="field">
          <span class="field-label">Message ID</span>
          <input v-model="mysql.filters.message_id" class="field-input" placeholder="模糊搜索" />
        </label>
        <label class="field">
          <span class="field-label">Sender ID</span>
          <input v-model="mysql.filters.sender_id" class="field-input" placeholder="模糊搜索" />
        </label>
        <label class="field">
          <span class="field-label">Sender Name</span>
          <input v-model="mysql.filters.sender_name" class="field-input" placeholder="模糊搜索" />
        </label>
        <label class="field">
          <span class="field-label">Group ID</span>
          <input v-model="mysql.filters.group_id" class="field-input" placeholder="模糊搜索" />
        </label>
        <label class="field">
          <span class="field-label">Content</span>
          <input v-model="mysql.filters.content" class="field-input" placeholder="模糊搜索" />
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
            <option v-for="c in redisConnections" :key="c.id" :value="c.id">{{ c.name }}</option>
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

    <!-- RustFS Tab -->
    <section v-if="activeTab === 'rustfs'" class="panel">
      <div class="explorer-connection-select">
        <label class="field">
          <span class="field-label">RustFS 连接</span>
          <select v-model="rustfs.connectionId" class="field-input" @change="onRustfsConnectionChange">
            <option value="">— 选择连接 —</option>
            <option v-for="c in rustfsConnections" :key="c.id" :value="c.id">{{ c.name }}</option>
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
  type MysqlRecord,
  type RedisKeyEntry,
  type RustfsObject,
} from "../../api/client";

const tabs = [
  { id: "mysql" as const, label: "MySQL" },
  { id: "redis" as const, label: "Redis" },
  { id: "rustfs" as const, label: "RustFS" },
];

const activeTab = ref<"mysql" | "redis" | "rustfs">("mysql");
const connections = ref<ConnectionConfig[]>([]);

const mysqlGoto = ref(1);
const redisGoto = ref(1);
const rustfsGoto = ref(1);

const mysqlConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "mysql" && c.enabled)
);
const redisConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "redis" && c.enabled)
);
const rustfsConnections = computed(() =>
  connections.value.filter((c) => c.kind.type === "rustfs" && c.enabled)
);

function switchTab(tab: "mysql" | "redis" | "rustfs") {
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
  try {
    connections.value = await system.connections.list();
  } catch {
    // silently fail
  }
});
</script>
