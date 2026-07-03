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
import { useCommands } from "../composables/useCommands";

const {
  commands,
  permissions,
  showEditor,
  editingId,
  form,
  loadData,
  permissionFor,
  scopeLabel,
  ruleLabel,
  ruleDetail,
  formatTime,
  startCreatePermission,
  editPermission,
  editExistingPermission,
  closeEditor,
  addRule,
  removeRule,
  onRuleTypeChange,
  syncAllowList,
  savePermission,
  deletePermission,
} = useCommands();
</script>

<style scoped lang="scss">
@use "../styles/commands" as *;
</style>
