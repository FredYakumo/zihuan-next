<template>
  <div class="component-install-page">
    <div class="page-heading"><div><h1>添加组件</h1><p>选择组件类型，配置安装方式与连接参数。</p></div><t-button variant="outline" :disabled="busy" @click="$emit('close')">返回列表</t-button></div>
    <t-alert v-if="error" theme="error" :message="error" class="page-alert" />
    <t-card :bordered="false">
      <template v-if="!request">
        <h3 class="picker-title">选择组件类型</h3>
        <t-loading :loading="loading">
          <div class="component-grid">
            <button v-for="item in options" :key="item.value" type="button" class="component-card" :disabled="loading" @click="select(item)">
              <img :src="item.icon" :alt="item.label"><strong>{{ item.label }}</strong><span>{{ item.description }}</span>
            </button>
          </div>
        </t-loading>
      </template>
      <template v-else>
        <div class="selected-component-bar"><img :src="selected.icon" :alt="selected.label"><div><strong>{{ selected.label }}</strong><span>{{ selected.description }}</span></div><t-button variant="text" :disabled="busy" @click="request = null">重新选择</t-button></div>
        <t-form label-align="top" class="component-install-form" @submit="submit">
          <div class="form-row">
            <t-form-item label="组件名称" required><t-input v-model="request.name" :disabled="busy" /></t-form-item>
            <t-form-item label="版本"><t-input v-model="request.version" :disabled="busy" /></t-form-item>
          </div>
          <t-form-item label="安装方式" required><t-radio-group v-model="request.install_method" :disabled="busy" :options="methods" @change="syncMethod" /></t-form-item>
          <template v-if="config && selected.value !== 'sqlite'">
            <div class="access-options"><t-checkbox v-model="config.expose_public_access" :disabled="busy">暴露公网访问</t-checkbox><t-checkbox v-if="config.expose_public_access" v-model="config.use_target_machine_address" :disabled="busy">使用目标机器地址</t-checkbox></div>
            <t-form-item v-if="config.expose_public_access && config.use_target_machine_address" label="目标机器地址"><t-input v-model="config.target_machine_address" :disabled="busy" placeholder="例如 203.0.113.10" /></t-form-item>
          </template>
          <t-divider v-if="fields.length" align="left">安装配置</t-divider>
          <div class="form-row">
            <t-form-item v-for="field in fields" :key="field.key" :label="field.label">
              <t-input-number v-if="field.type === 'number'" v-model="field.target[field.key]" :min="1" :max="field.max" :disabled="busy" />
              <t-switch v-else-if="field.type === 'boolean'" v-model="field.target[field.key]" :disabled="busy" />
              <t-input v-else :value="field.target[field.key] ?? ''" :type="field.type || 'text'" :disabled="busy" :placeholder="field.placeholder" @change="value => field.target[field.key] = value" />
            </t-form-item>
          </div>
          <t-alert v-if="hint" theme="info" :message="hint" />
        </t-form>
      </template>
    </t-card>
    <div class="install-actions"><t-button variant="outline" :disabled="busy" @click="$emit('close')">取消</t-button><t-button theme="primary" :disabled="!request || loading" :loading="busy" @click="submit">{{ request?.install_method.startsWith('command_') ? '生成安装命令' : '开始安装' }}</t-button></div>
  </div>
</template>

<script setup>
import { computed, ref } from 'vue';
import logo from '../src/assets/brand-icon.png';
import mysql from '../src/assets/icons/mysql.svg';
import sqlite from '../src/assets/icons/sqlite.svg';
import redis from '../src/assets/icons/redis.svg';
import weaviate from '../src/assets/icons/weaviate.svg';
import elasticsearch from '../src/assets/icons/elasticsearch.svg';
import rustfs from '../src/assets/icons/rustfs.svg';
import wechat from '../src/assets/icons/wechat.svg';
import qq from '../src/assets/icons/qq.svg';

const props = defineProps({ api: { type: Function, required: true } });
const emit = defineEmits(['close', 'installed']);
const request = ref(null), selected = ref(null), loading = ref(false), busy = ref(false), error = ref('');
const options = [
  { value: 'zihuan', label: '紫幻本体', description: '紫幻服务', icon: logo },
  { value: 'mysql', label: 'MySQL', description: '关系数据库', icon: mysql },
  { value: 'sqlite', label: 'SQLite3', description: '关系数据库', icon: sqlite },
  { value: 'redis', label: 'Redis', description: '缓存', icon: redis },
  { value: 'weaviate', label: 'Weaviate', description: '检索数据库', icon: weaviate },
  { value: 'elasticsearch', label: 'Elasticsearch', description: '检索数据库', icon: elasticsearch },
  { value: 'rustfs', label: 'RustFS', description: '对象存储', icon: rustfs },
  { value: 'wechat_pad', label: 'WeChatPadPro', description: '微信', icon: wechat },
  { value: 'napcat', label: 'NapCat', description: 'QQ', icon: qq },
];
const methodNames = { docker: '本机 Docker', native: '本机 native 安装', binary: '本机包管理器', embedded: '内嵌数据库', command_docker: 'Docker 安装命令', command_native: 'native 安装命令', command_binary: '包管理器安装命令' };
const methods = computed(() => {
  const kind = selected.value?.value;
  const allowed = kind === 'zihuan' ? ['docker', 'native'] : kind === 'sqlite' ? ['embedded'] : kind === 'wechat_pad' ? ['docker', 'command_docker'] : kind === 'napcat' ? ['docker', 'native', 'command_docker', 'command_native'] : ['docker', 'binary', 'command_docker', 'command_binary'];
  return allowed.map(value => ({ value, label: methodNames[value] }));
});
const config = computed(() => request.value?.detailed_config);
const fields = computed(() => {
  if (!request.value) return [];
  const result = [];
  function add(target, key, label, type, placeholder) { result.push({ target, key, label, type, placeholder, max: key === 'port' ? 65535 : undefined }); }
  const kind = selected.value.value;
  if (kind === 'zihuan') {
    const metadata = request.value.extra_install_metadata;
    if (request.value.install_method === 'docker') add(metadata, 'image', 'Docker 镜像');
    else { add(metadata, 'source', '发布目录或发布包 URL', 'text', '留空时下载官方发布包'); add(metadata, 'http_proxy', '下载代理', 'text', '例如 http://127.0.0.1:7897'); }
    add(metadata, 'host', '监听地址'); add(metadata, 'port', '端口', 'number');
    return result;
  }
  const section = config.value?.[{ mysql: 'relational', redis: 'redis', rustfs: 'rustfs', weaviate: 'search', elasticsearch: 'search' }[kind]];
  if (!section) return result;
  if (request.value.install_method.includes('docker')) {
    add(section.deployment, 'image', 'Docker 镜像'); add(section.deployment, 'container_name', '容器名称');
    add(section.deployment, 'data_dir', '宿主机数据目录（绝对路径覆盖数据卷）');
    add(section.deployment, 'restart_policy', '重启策略');
  }
  add(section.deployment, 'port', '端口', 'number');
  const definitions = {
    mysql: [['host', '主机'], ['username', '用户名'], ['password', '密码', 'password'], ['database', '数据库'], ['max_connections', '最大连接数', 'number'], ['acquire_timeout_secs', '获取连接超时（秒）', 'number']],
    redis: [['url', 'Redis URL'], ['username', '用户名'], ['password', '密码', 'password']],
    rustfs: [['endpoint', 'Endpoint'], ['bucket', 'Bucket'], ['region', 'Region'], ['access_key', 'Access Key'], ['secret_key', 'Secret Key', 'password'], ['public_base_url', '公开访问 URL'], ['path_style', '使用 path-style', 'boolean']],
    weaviate: [['base_url', 'Base URL'], ['api_key', 'API Key', 'password'], ['vector_dimensions', '向量维度', 'number']],
    elasticsearch: [['base_url', 'Base URL'], ['username', '用户名'], ['password', '密码', 'password'], ['vector_dimensions', '向量维度', 'number']],
  };
  for (const [key, label, type] of definitions[kind]) add(section, key, label, type);
  return result;
});
const hint = computed(() => {
  if (request.value?.install_method.startsWith('command_')) return '生成安装命令后，请在目标机器上执行。';
  return { sqlite: '在 Composer 数据目录中创建并初始化独立的 SQLite 数据库。', wechat_pad: '安装 WeChatPadPro、配套 MySQL、Redis 和 WebHook Docker 容器。', napcat: '安装 NapCat QQ Bot adapter，完成后可在组件详情中查看连接信息。', zihuan: '每个紫幻实例使用独立的数据目录。' }[selected.value?.value] || '安装会在当前本机执行，连接配置将保存到 Composer。';
});
async function select(item) {
  loading.value = true; error.value = '';
  try {
    const template = await props.api(`template/${item.value}`);
    const parsed = await props.api('parse-install', 'POST', { yaml: template.yaml });
    selected.value = item; request.value = parsed.request;
  } catch (failure) { error.value = failure.message || String(failure); }
  finally { loading.value = false; }
}
function syncMethod() { if (config.value) config.value.install_method = request.value.install_method.includes('docker') ? 'docker' : 'binary'; }
async function submit() {
  if (!request.value || busy.value) return;
  if (!request.value.name.trim()) { error.value = '请填写组件名称'; return; }
  busy.value = true; error.value = '';
  try { syncMethod(); const result = await props.api('components/install', 'POST', request.value); emit('installed', result); }
  catch (failure) { error.value = failure.message || String(failure); }
  finally { busy.value = false; }
}
</script>
