<template>
  <section class="page permissions-page">
    <AdminPageHeader title="权限组" />

    <t-card bordered>
      <div class="permission-toolbar">
        <t-select
          v-model="selectedGroup"
          class="permission-group-select"
          placeholder="选择权限组"
          filterable
          :loading="loadingGroups"
        >
          <t-option
            v-for="group in filteredGroups"
            :key="group.name"
            :value="group.name"
            :label="`${group.name}（${group.entry_count}）`"
          />
        </t-select>
        <t-input v-model="groupSearch" class="permission-search" placeholder="搜索权限组" clearable />
        <span class="muted">共 {{ groups.length }} 个权限组</span>
        <t-button theme="primary" class="permission-create-btn" @click="openCreateGroup">+ 新建权限组</t-button>
      </div>
      <t-alert v-if="groupsError" theme="error" :message="groupsError" />

      <div class="permission-entries-header">
        <div class="permission-entries-actions">
          <t-button variant="outline" :disabled="!selectedGroup" @click="openCreateEntry">+ 添加条目</t-button>
          <t-button variant="text" :loading="loadingEntries" @click="loadEntries">刷新</t-button>
        </div>
      </div>
      <t-alert v-if="entriesError" theme="error" :message="entriesError" />

      <div v-if="!selectedGroup" class="empty-state">请选择一个权限组</div>
      <t-table
        v-else
        :row-key="entryRowKey"
        bordered
        size="small"
        :data="entries"
        :columns="entryColumns"
        :loading="loadingEntries"
        empty="该权限组暂无条目"
      >
        <template #entity_id="{ row }">
          <t-tag v-if="row.entity_id === '*'" variant="light" theme="warning">所有用户 *</t-tag>
          <span v-else class="mono">{{ row.entity_id }}</span>
        </template>
        <template #entity_type="{ row }">
          <t-tag variant="light">{{ entityTypeLabel(row.entity_type) }}</t-tag>
        </template>
        <template #category="{ row }"><code>{{ row.category }}</code></template>
        <template #value="{ row }">
          <t-tag variant="light" :theme="row.value !== 0 ? 'success' : 'danger'">
            {{ row.value !== 0 ? "允许" : "拒绝" }}
          </t-tag>
        </template>
        <template #created_at="{ row }">{{ formatTime(row.created_at) }}</template>
        <template #updated_at="{ row }">{{ formatTime(row.updated_at) }}</template>
        <template #actions="{ row }">
          <t-button variant="text" size="small" @click="openEditEntry(row)">编辑</t-button>
          <t-popconfirm content="确认删除该权限条目？" @confirm="deleteEntry(row)">
            <t-button variant="text" theme="danger" size="small">删除</t-button>
          </t-popconfirm>
        </template>
      </t-table>
    </t-card>

    <t-dialog
      v-model:visible="groupDialogVisible"
      header="新建权限组"
      :close-on-overlay-click="false"
      width="420px"
    >
      <div class="permission-form-field">
        <label>权限组名</label>
        <t-input v-model="newGroupName" placeholder="例如 default、vip" @enter="createGroup" />
      </div>
      <t-alert v-if="groupDialogError" theme="error" :message="groupDialogError" />
      <template #footer>
        <t-button variant="outline" @click="groupDialogVisible = false">取消</t-button>
        <t-button theme="primary" :loading="savingGroup" @click="createGroup">创建</t-button>
      </template>
    </t-dialog>

    <t-dialog
      v-model:visible="entryDialogVisible"
      :header="editingEntry ? '编辑权限' : '添加权限'"
      :close-on-overlay-click="false"
      width="520px"
    >
      <div class="permission-form-grid">
        <div class="permission-form-field">
          <label>实体 ID</label>
          <t-input v-model="entryForm.entity_id" placeholder="用户 ID，输入 * 表示该类型的所有用户" :disabled="!!editingEntry" />
        </div>
        <div class="permission-form-field">
          <label>实体类型</label>
          <t-select v-model="entryForm.entity_type" :disabled="!!editingEntry">
            <t-option value="qq_id" label="qq_id" />
            <t-option value="wxid" label="wxid" />
          </t-select>
        </div>
        <div class="permission-form-field permission-form-field--wide">
          <label>权限类别</label>
          <t-input v-model="entryForm.category" :disabled="!!editingEntry" />
        </div>
        <div class="permission-form-field">
          <label>权限值</label>
          <t-select v-model="entryForm.value">
            <t-option :value="1" label="允许(1)" />
            <t-option :value="0" label="拒绝(0)" />
          </t-select>
        </div>
      </div>
      <t-alert v-if="entryDialogError" theme="error" :message="entryDialogError" />
      <template #footer>
        <t-button variant="outline" @click="entryDialogVisible = false">取消</t-button>
        <t-button theme="primary" :loading="savingEntry" @click="saveEntry">保存</t-button>
      </template>
    </t-dialog>

    <t-dialog
      v-model:visible="conflictVisible"
      header="条目已存在"
      :close-on-overlay-click="false"
      width="480px"
    >
      <p class="permission-conflict-message">{{ conflictMessage }}</p>
      <template #footer>
        <t-button variant="outline" @click="cancelConflict">取消</t-button>
        <t-button theme="primary" :loading="overwriting" @click="overwriteEntry">修改覆盖已有值</t-button>
      </template>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { usePermissions } from "./usePermissions";

const {
  groups,
  entries,
  selectedGroup,
  groupSearch,
  loadingGroups,
  loadingEntries,
  groupsError,
  entriesError,
  groupDialogVisible,
  newGroupName,
  groupDialogError,
  savingGroup,
  entryDialogVisible,
  editingEntry,
  entryForm,
  entryDialogError,
  savingEntry,
  conflictVisible,
  conflictMessage,
  overwriting,
  filteredGroups,
  entryColumns,
  loadEntries,
  openCreateGroup,
  createGroup,
  openCreateEntry,
  openEditEntry,
  saveEntry,
  overwriteEntry,
  cancelConflict,
  deleteEntry,
  entityTypeLabel,
  entryRowKey,
  formatTime,
} = usePermissions();
</script>

<style scoped lang="scss">
@use "./permissions" as *;
</style>
