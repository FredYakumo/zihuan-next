<template>
  <section class="page">
    <AdminPageHeader title="命令管理">
      <t-button theme="primary" @click="startCreatePermission">+ 添加权限规则</t-button>
    </AdminPageHeader>

    <t-card title="已注册命令" bordered header-bordered class="commands-card">
      <template #actions>
        <t-button variant="text" @click="loadData">刷新</t-button>
      </template>
      <p class="muted">共 {{ commands.length }} 个命令，由系统代码注册，不可在此增删。</p>

      <div v-if="commands.length === 0" class="empty-state">正在加载...</div>
      <t-table v-else row-key="name" bordered size="small" :data="commands" :columns="commandColumns">
        <template #name="{ row }">
          <span class="mono">/{{ row.name }}</span>
        </template>
        <template #aliases="{ row }">
          <span v-if="row.aliases.length">
            <code v-for="alias in row.aliases" :key="alias" class="tag">/{{ alias }}</code>
          </span>
          <span v-else class="muted">—</span>
        </template>
        <template #accepted_arg_count="{ row }">
          <code>{{ row.accepted_arg_count ?? 0 }}</code>
        </template>
        <template #scope="{ row }">
          <t-tag variant="light">{{ scopeLabel(row.scope) }}</t-tag>
        </template>
        <template #permission="{ row }">
          <t-tag v-if="permissionFor(row.name)" variant="light" :theme="permissionEnabled(row.name) ? 'success' : 'warning'">
            {{ permissionEnabled(row.name) ? "已配置" : "已禁用" }}
          </t-tag>
          <t-tag v-else variant="light" theme="default">默认（所有人）</t-tag>
        </template>
        <template #actions="{ row }">
          <t-button variant="text" size="small" @click="editPermission(row.name)">编辑权限</t-button>
        </template>
      </t-table>
    </t-card>

    <t-card title="权限规则" bordered header-bordered class="commands-card">
      <p class="muted">共 {{ permissions.length }} 条规则。权限规则控制谁能使用特定命令。</p>

      <div v-if="permissions.length === 0" class="empty-state">暂无自定义权限规则，所有命令默认对所有人开放。</div>
      <t-table v-else row-key="config_id" bordered size="small" :data="permissions" :columns="permissionColumns">
        <template #command_name="{ row }">
          <span class="mono">/{{ row.command_name }}</span>
        </template>
        <template #rules="{ row }">
          <div v-for="(rule, i) in row.rules" :key="i" class="rule-item">
            <t-tag variant="light">{{ ruleLabel(rule) }}</t-tag>
            <span class="mono rule-detail">{{ ruleDetail(rule) }}</span>
          </div>
        </template>
        <template #enabled="{ row }">
          <t-tag variant="light" :theme="row.enabled ? 'success' : 'danger'">{{ row.enabled ? "启用" : "禁用" }}</t-tag>
        </template>
        <template #updated_at="{ row }">{{ formatTime(row.updated_at) }}</template>
        <template #actions="{ row }">
          <t-button variant="text" size="small" @click="editExistingPermission(row)">编辑</t-button>
          <t-button variant="text" theme="danger" size="small" @click="deletePermission(row.config_id)">删除</t-button>
        </template>
      </t-table>
    </t-card>

    <t-dialog
      :visible="showEditor"
      :header="editingId ? '编辑权限规则' : '新建权限规则'"
      :close-on-overlay-click="false"
      attach="body"
      @close="closeEditor"
    >
      <template #body>
        <div class="form-grid">
          <div class="field">
            <label>命令名</label>
            <t-select v-model="form.command_name" placeholder="请选择命令">
              <t-option v-for="cmd in commands" :key="cmd.name" :value="cmd.name" :label="`/${cmd.name}`" />
            </t-select>
          </div>
          <div class="field-full status-row">
            <t-checkbox v-model="form.enabled">启用</t-checkbox>
          </div>
        </div>

        <div class="rules-section">
          <div class="split-header" style="margin-top: 12px">
            <h4>权限规则</h4>
            <t-button variant="text" @click="addRule">+ 添加规则</t-button>
          </div>
          <div v-if="form.rules.length === 0" class="empty-state" style="margin-top: 8px">
            尚未添加规则。默认行为：所有人可访问。
          </div>
          <div v-for="(rule, i) in form.rules" :key="i" class="rule-editor">
            <div class="field">
              <label>规则类型</label>
              <t-select v-model="rule.rule_type" @change="onRuleTypeChange(rule)">
                <t-option value="everyone" label="所有人" />
                <t-option value="qq_users" label="QQ 用户" />
                <t-option value="api_keys" label="API Key" />
              </t-select>
            </div>
            <div v-if="rule.rule_type !== 'everyone'" class="field-full">
              <label>允许列表（逗号分隔）</label>
              <t-input v-model="rule.allowListText" placeholder="例如: 123456,789012" @change="syncAllowList(rule)" />
            </div>
            <t-button variant="text" theme="danger" @click="removeRule(i)">删除规则</t-button>
          </div>
        </div>
      </template>
      <template #footer>
        <t-button theme="primary" @click="savePermission">{{ editingId ? "保存" : "创建" }}</t-button>
      </template>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useCommands } from "./useCommands";

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

function permissionEnabled(commandName: string): boolean {
  return permissionFor(commandName)?.enabled ?? false;
}

const commandColumns = [
  { colKey: "name", title: "命令名" },
  { colKey: "aliases", title: "别名" },
  { colKey: "accepted_arg_count", title: "参数数" },
  { colKey: "description", title: "描述" },
  { colKey: "scope", title: "适用范围" },
  { colKey: "permission", title: "权限状态" },
  { colKey: "actions", title: "操作" },
];

const permissionColumns = [
  { colKey: "command_name", title: "命令" },
  { colKey: "rules", title: "规则" },
  { colKey: "enabled", title: "状态" },
  { colKey: "updated_at", title: "更新时间" },
  { colKey: "actions", title: "操作" },
];
</script>

<style scoped lang="scss">
@use "./commands" as *;
</style>
