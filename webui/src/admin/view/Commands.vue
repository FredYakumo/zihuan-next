<template>
  <section class="page">
    <div class="page-hero">
      <h2>命令管理</h2>
      <div class="hero-actions">
        <button class="btn primary" @click="startCreatePermission">+ 添加权限规则</button>
      </div>
    </div>

    <!-- Registered commands table -->
    <section class="panel">
      <div class="split-header">
        <div>
          <h3>已注册命令</h3>
          <p class="muted">共 {{ commands.length }} 个命令，由系统代码注册，不可在此增删。</p>
        </div>
        <button class="btn ghost" @click="loadData">刷新</button>
      </div>

      <div v-if="commands.length === 0" class="empty-state">正在加载...</div>

      <table v-else class="data-table">
        <thead>
          <tr>
            <th>命令名</th>
            <th>别名</th>
            <th>参数数</th>
            <th>描述</th>
            <th>适用范围</th>
            <th>权限状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="cmd in commands" :key="cmd.name">
            <td class="mono">/{{ cmd.name }}</td>
            <td>
              <span v-if="cmd.aliases.length">
                <code v-for="alias in cmd.aliases" :key="alias" class="tag">/{{ alias }}</code>
              </span>
              <span v-else class="muted">—</span>
            </td>
            <td>
              <code>{{ cmd.accepted_arg_count ?? 0 }}</code>
            </td>
            <td>{{ cmd.description }}</td>
            <td>
              <span class="badge">{{ scopeLabel(cmd.scope) }}</span>
            </td>
            <td>
              <span v-if="permissionFor(cmd.name)" class="badge" :class="permissionFor(cmd.name).enabled ? 'badge-success' : 'badge-warning'">
                {{ permissionFor(cmd.name).enabled ? '已配置' : '已禁用' }}
              </span>
              <span v-else class="badge badge-muted">默认（所有人）</span>
            </td>
            <td>
              <button class="btn ghost" @click="editPermission(cmd.name)">编辑权限</button>
            </td>
          </tr>
        </tbody>
      </table>
    </section>

    <!-- Permission rules -->
    <section class="panel">
      <div class="split-header">
        <div>
          <h3>权限规则</h3>
          <p class="muted">共 {{ permissions.length }} 条规则。权限规则控制谁能使用特定命令。</p>
        </div>
      </div>

      <div v-if="permissions.length === 0" class="empty-state">暂无自定义权限规则，所有命令默认对所有人开放。</div>

      <table v-else class="data-table">
        <thead>
          <tr>
            <th>命令</th>
            <th>规则</th>
            <th>状态</th>
            <th>更新时间</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="perm in permissions" :key="perm.config_id">
            <td class="mono">/{{ perm.command_name }}</td>
            <td>
              <div v-for="(rule, i) in perm.rules" :key="i" class="rule-item">
                <span class="badge">{{ ruleLabel(rule) }}</span>
                <span class="mono rule-detail">{{ ruleDetail(rule) }}</span>
              </div>
            </td>
            <td>
              <span class="badge" :class="perm.enabled ? 'badge-success' : 'badge-danger'">
                {{ perm.enabled ? '启用' : '禁用' }}
              </span>
            </td>
            <td>{{ formatTime(perm.updated_at) }}</td>
            <td>
              <button class="btn ghost" @click="editExistingPermission(perm)">编辑</button>
              <button class="btn ghost danger" @click="deletePermission(perm.config_id)">删除</button>
            </td>
          </tr>
        </tbody>
      </table>
    </section>

    <!-- Create / Edit Modal -->
    <div v-if="showEditor" class="connection-picker-backdrop">
      <div class="connection-picker-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>{{ editingId ? '编辑权限规则' : '新建权限规则' }}</h3>
          <button class="btn ghost" @click="closeEditor">取消</button>
        </div>
        <div class="connection-picker-form">
          <div class="form-grid">
            <div class="field">
              <label>命令名</label>
              <select v-model="form.command_name">
                <option value="">请选择命令</option>
                <option v-for="cmd in commands" :key="cmd.name" :value="cmd.name">
                  /{{ cmd.name }}
                </option>
              </select>
            </div>
            <div class="field-full status-row">
              <label class="field-check">
                <input v-model="form.enabled" type="checkbox" />启用
              </label>
            </div>
          </div>

          <div class="rules-section">
            <div class="split-header" style="margin-top: 12px;">
              <h4>权限规则</h4>
              <button class="btn ghost" @click="addRule">+ 添加规则</button>
            </div>
            <div v-if="form.rules.length === 0" class="empty-state" style="margin-top: 8px;">
              尚未添加规则。默认行为：所有人可访问。
            </div>
            <div v-for="(rule, i) in form.rules" :key="i" class="rule-editor">
              <div class="field">
                <label>规则类型</label>
                <select v-model="rule.rule_type" @change="onRuleTypeChange(rule)">
                  <option value="everyone">所有人</option>
                  <option value="qq_users">QQ 用户</option>
                  <option value="api_keys">API Key</option>
                </select>
              </div>
              <div v-if="rule.rule_type !== 'everyone'" class="field-full">
                <label>允许列表（逗号分隔）</label>
                <input
                  v-model="rule.allowListText"
                  placeholder="例如: 123456,789012"
                  @input="syncAllowList(rule)"
                />
              </div>
              <button class="btn ghost danger" @click="removeRule(i)">删除规则</button>
            </div>
          </div>

          <div class="dialog-actions" style="margin-top: 16px;">
            <button class="btn primary" @click="savePermission">
              {{ editingId ? '保存' : '创建' }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface CommandDef {
  name: string;
  aliases: string[];
  description: string;
  scope: string | { Specific?: { agent_ids: string[] } };
}

interface PermissionRule {
  rule_type: string;
  allowed_ids?: string[];
  allowed_keys?: string[];
  allow_list?: string[];
}

interface CommandPermission {
  config_id: string;
  command_name: string;
  rules: PermissionRule[];
  enabled: boolean;
  updated_at: string;
}

interface RuleForm {
  rule_type: string;
  allowListText: string;
}

interface PermissionForm {
  command_name: string;
  enabled: boolean;
  rules: RuleForm[];
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const commands = ref<CommandDef[]>([]);
const permissions = ref<CommandPermission[]>([]);
const showEditor = ref(false);
const editingId = ref<string | null>(null);

const form = ref<PermissionForm>({
  command_name: "",
  enabled: true,
  rules: [],
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

onMounted(() => {
  loadData();
});

// ---------------------------------------------------------------------------
// Data loading
// ---------------------------------------------------------------------------

async function loadData() {
  await Promise.all([loadCommands(), loadPermissions()]);
}

async function loadCommands() {
  try {
    const res = await fetch("/api/system/commands/registry");
    if (res.ok) {
      commands.value = await res.json();
    }
  } catch (e) {
    console.error("Failed to load commands registry", e);
  }
}

async function loadPermissions() {
  try {
    const res = await fetch("/api/system/command-permissions");
    if (res.ok) {
      permissions.value = await res.json();
    }
  } catch (e) {
    console.error("Failed to load command permissions", e);
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function permissionFor(commandName: string): CommandPermission | undefined {
  return permissions.value.find((p) => p.command_name === commandName);
}

function scopeLabel(scope: any): string {
  if (typeof scope === "string") {
    const map: Record<string, string> = {
      all: "全部",
      qq_chat: "QQ Chat",
      http_stream: "HTTP Stream",
    };
    return map[scope] ?? scope;
  }
  if (scope?.Specific) {
    return `指定 Agent（${scope.Specific.agent_ids?.join(", ") ?? ""}）`;
  }
  return "全部";
}

function ruleLabel(rule: PermissionRule): string {
  const map: Record<string, string> = {
    everyone: "所有人",
    qq_users: "QQ 用户",
    api_keys: "API Key",
    custom: "自定义",
  };
  return map[rule.rule_type] ?? rule.rule_type;
}

function ruleDetail(rule: PermissionRule): string {
  if (rule.rule_type === "everyone") return "";
  const ids = rule.allowed_ids ?? rule.allowed_keys ?? rule.allow_list ?? [];
  return ids.join(", ");
}

function formatTime(ts: string): string {
  if (!ts) return "-";
  try {
    return new Date(ts).toLocaleString();
  } catch {
    return ts;
  }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

function startCreatePermission() {
  editingId.value = null;
  form.value = {
    command_name: "",
    enabled: true,
    rules: [],
  };
  showEditor.value = true;
}

function editPermission(commandName: string) {
  const existing = permissionFor(commandName);
  if (existing) {
    editExistingPermission(existing);
    return;
  }
  editingId.value = null;
  form.value = {
    command_name: commandName,
    enabled: true,
    rules: [],
  };
  showEditor.value = true;
}

function editExistingPermission(perm: CommandPermission) {
  editingId.value = perm.config_id;
  form.value = {
    command_name: perm.command_name,
    enabled: perm.enabled,
    rules: perm.rules.map((r) => ({
      rule_type: r.rule_type,
      allowListText: (r.allowed_ids ?? r.allowed_keys ?? r.allow_list ?? []).join(", "),
    })),
  };
  showEditor.value = true;
}

function closeEditor() {
  showEditor.value = false;
  editingId.value = null;
}

function addRule() {
  form.value.rules.push({
    rule_type: "everyone",
    allowListText: "",
  });
}

function removeRule(index: number) {
  form.value.rules.splice(index, 1);
}

function onRuleTypeChange(rule: RuleForm) {
  if (rule.rule_type === "everyone") {
    rule.allowListText = "";
  }
}

function syncAllowList(rule: RuleForm) {
  // handled by v-model
}

function buildApiRules(): PermissionRule[] {
  return form.value.rules.map((r) => {
    const ids = r.allowListText
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    if (r.rule_type === "qq_users") {
      return { rule_type: "qq_users", allowed_ids: ids };
    } else if (r.rule_type === "api_keys") {
      return { rule_type: "api_keys", allowed_keys: ids };
    }
    return { rule_type: "everyone" };
  });
}

async function savePermission() {
  const body = {
    command_name: form.value.command_name,
    enabled: form.value.enabled,
    rules: buildApiRules(),
  };

  try {
    let res: Response;
    if (editingId.value) {
      res = await fetch(`/api/system/command-permissions/${editingId.value}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    } else {
      res = await fetch("/api/system/command-permissions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    }

    if (!res.ok) {
      const err = await res.json();
      alert(`保存失败: ${err.error ?? res.statusText}`);
      return;
    }

    await loadPermissions();
    closeEditor();
  } catch (e) {
    alert(`请求失败: ${e}`);
  }
}

async function deletePermission(configId: string) {
  if (!confirm("确定删除此权限规则？删除后将恢复默认（所有人可访问）。")) return;

  try {
    const res = await fetch(`/api/system/command-permissions/${configId}`, {
      method: "DELETE",
    });
    if (!res.ok) {
      const err = await res.json();
      alert(`删除失败: ${err.error ?? res.statusText}`);
      return;
    }
    await loadPermissions();
  } catch (e) {
    alert(`请求失败: ${e}`);
  }
}
</script>

<style scoped>
.data-table {
  width: 100%;
  border-collapse: collapse;
}
.data-table th,
.data-table td {
  text-align: left;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-light, #e0e0e0);
}
.rule-item {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}
.rule-detail {
  font-size: 0.85em;
  color: var(--text-muted, #666);
}
.rules-section {
  border-top: 1px solid var(--border-light, #e0e0e0);
  padding-top: 8px;
}
.rule-editor {
  display: flex;
  align-items: flex-end;
  gap: 12px;
  padding: 8px 0;
}
.rule-editor .field {
  flex: 1;
}
.tag {
  display: inline-block;
  margin-right: 4px;
}
.badge-success {
  background: var(--success-bg, #d4edda);
  color: var(--success-text, #155724);
}
.badge-warning {
  background: var(--warning-bg, #fff3cd);
  color: var(--warning-text, #856404);
}
.badge-danger {
  background: var(--danger-bg, #f8d7da);
  color: var(--danger-text, #721c24);
}
.badge-muted {
  background: var(--bg-muted, #e9ecef);
  color: var(--text-muted, #6c757d);
}
.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.mono {
  font-family: monospace;
}
.muted {
  color: var(--text-muted, #6c757d);
}
</style>
