<template>
  <t-layout class="composer-shell">
    <t-aside width="216px" class="composer-aside">
      <div class="brand"><img :src="logo" alt="紫幻"><div>Zihuan Composer<small>组件管理</small></div></div>
      <t-menu class="composer-menu" width="100%" :value="page" @change="changePage">
        <t-menu-item value="components"><template #icon><ServerIcon /></template>组件管理</t-menu-item>
        <t-menu-item value="environment"><template #icon><ControlPlatformIcon /></template>运行环境</t-menu-item>
      </t-menu>
      <div v-if="buildInfo" class="aside-footer">
        <span>Zihuan Composer · v{{ buildInfo.composer_version }}</span>
        <span class="aside-build-info" :title="`zihuan_next ${buildInfo.version} ${buildInfo.build_type}`">zihuan_next {{ buildInfo.version }} {{ buildInfo.build_type }}</span>
      </div>
    </t-aside>
    <t-layout>
      <t-header class="composer-header"><t-breadcrumb><t-breadcrumb-item>Composer</t-breadcrumb-item><t-breadcrumb-item>{{ titles[page] }}</t-breadcrumb-item><t-breadcrumb-item v-if="installVisible">添加组件</t-breadcrumb-item></t-breadcrumb></t-header>
      <t-content class="composer-content">
        <ComponentInstall v-if="installVisible" :api="api" @close="installVisible = false" @installed="result => guard(() => installed(result))" />
        <template v-else>
        <div class="page-heading"><div><h1>{{ titles[page] }}</h1><p>{{ descriptions[page] }}</p></div><t-space><t-button v-if="page === 'components'" variant="outline" @click="guard(() => openExport())"><template #icon><DownloadIcon /></template>导出全部连接配置</t-button><t-button variant="outline" :loading="loading" @click="guard(refreshPage)"><template #icon><RefreshIcon /></template>刷新</t-button><t-button v-if="page === 'components'" theme="primary" @click="guard(openInstall)"><template #icon><AddIcon /></template>添加组件</t-button></t-space></div>
        <t-alert v-if="error" theme="error" :message="error" close @close="error = ''" class="page-alert" />
        <template v-if="page === 'components'">
          <div class="overview"><t-card v-for="stat in stats" :key="stat.label" :bordered="false"><span>{{ stat.label }}</span><strong>{{ stat.value }}</strong><small>{{ stat.hint }}</small></t-card></div>
          <t-card :bordered="false" class="list-card">
            <div class="table-toolbar"><t-input v-model="keyword" placeholder="搜索组件名称或类型" clearable><template #prefix-icon><SearchIcon /></template></t-input><t-select v-model="statusFilter" :options="statusOptions" /></div>
            <t-table :data="filtered" :columns="columns" row-key="id" :loading="loading" hover stripe>
              <template #name="{ row }"><div class="component-name"><div class="component-icon"><ServerIcon /></div><div><strong>{{ row.name }}</strong><small>{{ names[row.component_type] || row.component_type }}</small></div></div></template>
              <template #installation_method="{ row }"><t-tag variant="light">{{ methodNames[row.installation_method] || row.installation_method }}</t-tag></template>
              <template #status="{ row }"><t-tag :theme="statusTheme(row.status)" variant="light">{{ statuses[row.status] || row.status }}</t-tag></template>
              <template #actions="{ row }"><div class="component-actions"><t-button variant="text" @click="openDetails(row)">详情</t-button><t-button v-if="configurable(row)" variant="text" @click="guard(() => openConfig(row))">配置</t-button><t-button v-if="['installed', 'disabled'].includes(row.status)" variant="text" @click="guard(() => toggle(row))">{{ row.status === 'installed' ? '停止' : '启动' }}</t-button><t-button v-if="row.status === 'installing'" variant="text" @click="guard(() => watchTask(row.extra_install_metadata.install_task_id))">进度</t-button><t-popconfirm v-else content="确认卸载此组件？原安装器管理的数据也可能被删除。" @confirm="guard(() => remove(row))"><t-button variant="text" theme="danger">卸载</t-button></t-popconfirm><template v-if="row.connection_ids?.length"><t-button variant="text" @click="guard(() => openExport(row))">导出连接</t-button><t-button variant="text" @click="guard(() => copyComponentConnections(row))">复制 JSON</t-button></template><t-button v-if="row.extra_install_metadata?.install_command" variant="text" @click="guard(() => copyText(row.extra_install_metadata.install_command))">复制命令</t-button></div></template>
              <template #empty><div class="empty-state"><ServerIcon /><h3>{{ components.length ? '没有匹配的组件' : '还没有安装组件' }}</h3><p>添加紫幻本体或所需的数据库、存储与 Bot 服务。</p><t-button v-if="!components.length" theme="primary" @click="guard(openInstall)"><template #icon><AddIcon /></template>添加第一个组件</t-button></div></template>
            </t-table>
          </t-card>
        </template>
        <t-loading v-else :loading="loading"><t-card title="系统环境" :bordered="false"><t-descriptions v-if="environment" bordered :column="2"><t-descriptions-item label="操作系统">{{ environment.os_detail || environment.os }}</t-descriptions-item><t-descriptions-item label="Docker"><t-tag :theme="environment.docker_available ? 'success' : 'default'" variant="light">{{ environment.docker_available ? '已安装' : '不可用' }}</t-tag></t-descriptions-item><t-descriptions-item label="Docker Compose">{{ environment.docker_compose_available ? '可用' : '不可用' }}</t-descriptions-item><t-descriptions-item label="native 安装">{{ environment.binary_install_available ? '可用' : '不可用' }}</t-descriptions-item><t-descriptions-item label="编译器">{{ environment.compiler_version || '未检测到' }}</t-descriptions-item><t-descriptions-item label="代理">{{ environment.proxy || '未检测到' }}</t-descriptions-item></t-descriptions><t-divider>本地服务检测</t-divider>
          <t-table :data="environment?.services || []" :columns="serviceColumns" row-key="service">
            <template #address="{ row }"><button v-if="row.detected" type="button" class="service-endpoint" :title="`点击复制 ${row.address}`" :aria-label="`复制连接地址 ${row.address}`" @click="guard(() => copyText(row.address))"><span class="service-endpoint-icon" aria-hidden="true">⧉</span><span class="service-endpoint-address">{{ row.address }}</span></button><span v-else class="service-endpoint-missing">未检测到</span></template>
          </t-table></t-card></t-loading>
        </template>
      </t-content>
    </t-layout>
    <t-dialog v-model:visible="exportVisible" :header="exportTitle" width="800px" :confirm-btn="null" :close-on-overlay-click="false">
      <t-alert theme="info" message="可临时编辑本次导出的连接配置，修改不会写回组件。导出的 JSON 可在紫幻本体中导入。" />
      <div class="export-toolbar"><span>{{ exportEditing ? '编辑连接配置 JSON' : '连接配置 JSON' }}</span><t-button variant="outline" @click="exportEditing = !exportEditing">{{ exportEditing ? '完成编辑' : '编辑连接配置' }}</t-button></div>
      <t-alert v-if="exportError" theme="error" :message="exportError" />
      <t-textarea v-model="exportJson" class="yaml-editor export-editor" :readonly="!exportEditing" :autosize="{ minRows: 14, maxRows: 22 }" @change="exportError = ''" />
      <template #footer><t-button variant="outline" @click="runExport('copy')">复制</t-button><t-button theme="primary" @click="runExport('save')">保存为 JSON</t-button><t-button variant="outline" @click="exportVisible = false">取消</t-button></template>
    </t-dialog>
    <t-dialog v-model:visible="configVisible" header="组件配置" width="800px" :confirm-btn="null"><t-alert v-if="dialogError" theme="error" :message="dialogError" /><p class="config-path">{{ configPath }}</p><t-textarea v-model="configYaml" class="yaml-editor" :autosize="{ minRows: 18, maxRows: 26 }" /><template #footer><t-button variant="outline" @click="configVisible = false">取消</t-button><t-button :loading="busy" variant="outline" @click="guard(() => saveConfig(false))">保存</t-button><t-button theme="primary" :loading="busy" @click="guard(() => saveConfig(true))">保存并重新启动</t-button></template></t-dialog>
    <t-dialog v-model:visible="detailsVisible" :header="selected?.name || '组件详情'" width="800px" :confirm-btn="null" @close="stopLogin"><t-alert v-if="dialogError" theme="error" :message="dialogError" /><t-descriptions v-if="selected" bordered :column="2"><t-descriptions-item label="组件">{{ names[selected.component_type] }}</t-descriptions-item><t-descriptions-item label="版本">{{ selected.version }}</t-descriptions-item><t-descriptions-item label="安装方式">{{ methodNames[selected.installation_method] }}</t-descriptions-item><t-descriptions-item label="状态">{{ statuses[selected.status] }}</t-descriptions-item></t-descriptions><t-collapse class="details-metadata"><t-collapse-panel header="部署信息与配置" value="metadata"><pre>{{ JSON.stringify(selected, null, 2) }}</pre></t-collapse-panel></t-collapse><template v-if="selected?.extra_install_metadata.install_command"><h4>安装命令</h4><pre>{{ selected.extra_install_metadata.install_command }}</pre></template><div v-if="selected?.component_type === 'wechat_pad'" class="login-panel"><t-button @click="guard(requestQr)">获取微信登录二维码</t-button><img v-if="qrImage" :src="qrImage" alt="微信登录二维码"><a v-if="qrLink" :href="qrLink" target="_blank" rel="noreferrer">打开登录链接</a><p>{{ loginStatus }}</p></div></t-dialog>
    <t-dialog v-model:visible="progressVisible" header="任务进度" width="760px" :confirm-btn="null" cancel-btn="关闭"><t-alert v-if="taskError" theme="error" :message="taskError" /><div class="task-console"><div v-if="!events.length" class="muted">等待任务启动…</div><div v-for="(event, index) in events" :key="index" :class="event.status"><span>[{{ event.step }}]</span> {{ event.message }}</div></div></t-dialog>
  </t-layout>
</template>

<script setup>
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';
import { MessagePlugin } from 'tdesign-vue-next';
import { AddIcon, RefreshIcon, SearchIcon, ServerIcon, DownloadIcon, ControlPlatformIcon } from 'tdesign-icons-vue-next';
import logo from '../src/assets/brand-icon.png';
import ComponentInstall from './ComponentInstall.vue';

const initialToken = location.hash.slice(1);
if (initialToken) { sessionStorage.setItem('composer-token', initialToken); history.replaceState(null, '', '/'); }
const token = sessionStorage.getItem('composer-token');
const buildInfo = ref(null);
const names = { zihuan: '紫幻本体', mysql: 'MySQL', sqlite: 'SQLite', redis: 'Redis', weaviate: 'Weaviate', elasticsearch: 'Elasticsearch', rustfs: 'RustFS', napcat: 'NapCat', wechat_pad: 'WeChatPadPro' };
const statuses = { installed: '已启用', disabled: '已停止', failed: '失败', installing: '安装中', command_generated: '命令已生成' };
const methodNames = { docker: 'Docker', native: 'native', binary: '本机包管理器', embedded: '内嵌数据库', command_docker: 'Docker 安装命令', command_native: 'native 安装命令', command_binary: '包管理器安装命令' };
const titles = { components: '组件管理', environment: '运行环境' };
const descriptions = { components: '管理紫幻本体与依赖服务的安装、配置和运行状态。', environment: '查看宿主机的 Docker、native 安装能力及本地服务。' };
const page = ref('components'), components = ref([]), loading = ref(false), busy = ref(false), error = ref(''), dialogError = ref('');
const keyword = ref(''), statusFilter = ref('all'), environment = ref(null);
const exportVisible = ref(false), exportTitle = ref(''), exportJson = ref(''), exportEditing = ref(false), exportError = ref(''), exportFilename = ref('');
const installVisible = ref(false);
const configVisible = ref(false), configYaml = ref(''), configPath = ref(''), selected = ref(null), detailsVisible = ref(false);
const progressVisible = ref(false), events = ref([]), taskError = ref(''), qrImage = ref(''), qrLink = ref(''), loginStatus = ref('');
let loginTimer, disposed = false;
const pendingTasks = new Set();
const statusOptions = [{ value: 'all', label: '全部状态' }, ...Object.entries(statuses).map(([value, label]) => ({ value, label }))];
const columns = [{ colKey: 'name', title: '组件名称', minWidth: 220 }, { colKey: 'version', title: '版本', width: 110 }, { colKey: 'installation_method', title: '安装方式', width: 160 }, { colKey: 'status', title: '状态', width: 120 }, { colKey: 'actions', title: '操作', width: 270 }];
const serviceColumns = [{ colKey: 'service', title: '服务' }, { colKey: 'address', title: '连接地址' }];
const filtered = computed(() => components.value.filter(item => (statusFilter.value === 'all' || item.status === statusFilter.value) && `${item.name} ${names[item.component_type]}`.toLowerCase().includes(keyword.value.toLowerCase())));
const stats = computed(() => [{ label: '全部组件', value: components.value.length, hint: '本体与依赖服务' }, { label: '已启用', value: components.value.filter(item => item.status === 'installed').length, hint: '已完成安装并启用' }, { label: '需要关注', value: components.value.filter(item => ['failed', 'installing'].includes(item.status)).length, hint: '安装中或安装失败' }]);
const statusTheme = status => ({ installed: 'success', failed: 'danger', installing: 'primary', disabled: 'warning' }[status] || 'default');
const configurable = item => item.installation_method === 'docker' || (item.component_type === 'zihuan' && item.installation_method === 'native');
async function api(path, method = 'GET', body) {
  const response = await fetch(`/api/${path}`, { method, headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || response.statusText);
  return value;
}
async function guard(action) { try { await action(); } catch (failure) { const message = failure instanceof Error ? failure.message : String(failure); error.value = message; dialogError.value = message; } }
async function refresh() { components.value = await api('components'); }
async function refreshPage() { loading.value = true; error.value = ''; try { if (page.value === 'components') await refresh(); else environment.value = await api('environment'); } finally { loading.value = false; } }
function changePage(value) { installVisible.value = false; page.value = value; guard(refreshPage); }
function openInstall() { error.value = ''; installVisible.value = true; }
async function installed(result) { installVisible.value = false; await refresh(); if (result.task_id) await watchTask(result.task_id); else if (result.component) openDetails(result.component); }
async function toggle(item) { const result = await api(`components/${item.id}/${item.status === 'installed' ? 'disable' : 'enable'}`, 'POST'); if (result.command) openDetails({ ...item, extra_install_metadata: { ...item.extra_install_metadata, install_command: result.command } }); await refresh(); }
async function remove(item) { const result = await api(`components/${item.id}`, 'DELETE'); if (result.task_id) await watchTask(result.task_id); await refresh(); }
async function watchTask(id) { progressVisible.value = true; if (pendingTasks.has(id)) return; pendingTasks.add(id); taskError.value = ''; events.value = []; try { while (!disposed) { events.value = await api(`tasks/${id}`); const terminal = events.value.find(event => event.step === 'finished' || event.status === 'error'); if (terminal) { if (terminal.status === 'error') taskError.value = terminal.message; await refresh(); return; } await new Promise(resolve => setTimeout(resolve, 500)); } } finally { pendingTasks.delete(id); } }
function openDetails(item) { stopLogin(); dialogError.value = ''; selected.value = item; qrImage.value = ''; qrLink.value = ''; loginStatus.value = ''; detailsVisible.value = true; }
async function openConfig(item) { dialogError.value = ''; const result = await api(`components/${item.id}/configuration`); selected.value = item; configYaml.value = result.yaml; configPath.value = result.path; configVisible.value = true; }
async function saveConfig(restart) { busy.value = true; try { await api(`components/${selected.value.id}/configuration`, 'PUT', { yaml: configYaml.value }); if (restart) { await api(`components/${selected.value.id}/disable`, 'POST'); await api(`components/${selected.value.id}/enable`, 'POST'); await refresh(); } configVisible.value = false; MessagePlugin.success('配置已保存'); } finally { busy.value = false; } }
async function componentConnections(item) { const connections = await api('connections'); return item ? connections.filter(connection => item.connection_ids?.includes(connection.config_id) || item.connection_ids?.includes(connection.id)) : connections; }
async function openExport(item) { const connections = await componentConnections(item); exportTitle.value = item ? `${item.name} · 导出连接配置` : '导出全部连接配置'; exportFilename.value = item ? `zihuan-connections-${item.id}.json` : 'zihuan-connections.json'; exportJson.value = JSON.stringify(connections, null, 2); exportEditing.value = false; exportError.value = ''; exportVisible.value = true; }
async function copyText(text) { await navigator.clipboard.writeText(text); MessagePlugin.success('已复制'); }
async function copyComponentConnections(item) { await copyText(JSON.stringify(await componentConnections(item), null, 2)); }
async function runExport(action) {
  exportError.value = '';
  try {
    const value = JSON.parse(exportJson.value);
    if (!Array.isArray(value) || value.some(connection => !connection || typeof connection !== 'object' || Array.isArray(connection) || !connection.config_id || !connection.kind)) throw new Error('连接配置必须是包含 config_id 和 kind 的 JSON 对象数组');
    const text = JSON.stringify(value, null, 2);
    if (action === 'copy') await copyText(text);
    else { const url = URL.createObjectURL(new Blob([text], { type: 'application/json' })); const anchor = document.createElement('a'); anchor.href = url; anchor.download = exportFilename.value; anchor.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); }
  } catch (failure) { exportError.value = failure instanceof SyntaxError ? 'JSON 格式不正确，请检查后重试' : failure.message || String(failure); }
}
function stopLogin() { clearTimeout(loginTimer); }
async function requestQr() { stopLogin(); const id = selected.value.id; const result = await api('components/wechat_pad/login_qr', 'POST', { component_id: id }); if (result.qr_base64) qrImage.value = result.qr_base64.startsWith('data:') ? result.qr_base64 : `data:image/png;base64,${result.qr_base64}`; if (/^https?:\/\//.test(result.qr_link || '')) qrLink.value = result.qr_link; async function poll() { const status = await api('components/wechat_pad/login_status', 'POST', { component_id: id, auth_key: result.auth_key || '' }); loginStatus.value = status.message || status.status; if (!['logged_in', 'expired', 'error'].includes(status.status) && detailsVisible.value) loginTimer = setTimeout(() => guard(poll), 2000); } if (result.status === 'logged_in') loginStatus.value = '已登录'; else loginTimer = setTimeout(() => guard(poll), 1500); }
onMounted(() => { guard(async () => { buildInfo.value = await api('build-info'); }); guard(refreshPage); });
onBeforeUnmount(() => { disposed = true; stopLogin(); });
</script>
