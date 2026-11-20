import { computed, onMounted, ref, watch } from "vue";

export interface PermissionGroup {
  name: string;
  created_at: string;
  updated_at: string;
  entry_count: number;
}

export interface PermissionEntry {
  group_name: string;
  entity_id: string;
  entity_type: string;
  category: string;
  value: number;
  created_at: string;
  updated_at: string;
}

interface EntryForm {
  entity_id: string;
  entity_type: string;
  category: string;
  value: number;
}

const emptyEntryForm = (): EntryForm => ({
  entity_id: "",
  entity_type: "qq_id",
  category: "",
  value: 1,
});

export function usePermissions() {
  const groups = ref<PermissionGroup[]>([]);
  const entries = ref<PermissionEntry[]>([]);
  const selectedGroup = ref("");
  const groupSearch = ref("");
  const loadingGroups = ref(false);
  const loadingEntries = ref(false);
  const groupsError = ref("");
  const entriesError = ref("");

  const groupDialogVisible = ref(false);
  const newGroupName = ref("");
  const groupDialogError = ref("");
  const savingGroup = ref(false);

  const entryDialogVisible = ref(false);
  const editingEntry = ref<PermissionEntry | null>(null);
  const entryForm = ref<EntryForm>(emptyEntryForm());
  const entryDialogError = ref("");
  const savingEntry = ref(false);

  const conflictVisible = ref(false);
  const conflictEntry = ref<PermissionEntry | null>(null);
  const conflictMessage = ref("");
  const pendingOverwrite = ref<EntryForm | null>(null);
  const overwriting = ref(false);

  const filteredGroups = computed(() => {
    const keyword = groupSearch.value.trim().toLowerCase();
    if (!keyword) return groups.value;
    return groups.value.filter((group) => group.name.toLowerCase().includes(keyword));
  });

  const groupColumns = [
    { colKey: "name", title: "权限组名" },
    { colKey: "entry_count", title: "条目数" },
    { colKey: "updated_at", title: "更新时间" },
  ];

  const entryColumns = [
    { colKey: "entity_id", title: "实体 ID" },
    { colKey: "entity_type", title: "实体类型" },
    { colKey: "category", title: "权限类别" },
    { colKey: "value", title: "权限值" },
    { colKey: "created_at", title: "创建时间" },
    { colKey: "updated_at", title: "更新时间" },
    { colKey: "actions", title: "操作" },
  ];

  onMounted(() => {
    void loadGroups();
  });

  watch(selectedGroup, () => {
    void loadEntries();
  });

  async function loadGroups() {
    loadingGroups.value = true;
    groupsError.value = "";
    try {
      const res = await fetch("/api/system/permission-groups");
      if (!res.ok) {
        groupsError.value = await errorText(res);
        return;
      }
      groups.value = await res.json();
      if (!selectedGroup.value || !groups.value.some((group) => group.name === selectedGroup.value)) {
        const preferred =
          groups.value.find((group) => group.name === "default") ?? groups.value[0];
        selectedGroup.value = preferred?.name ?? "";
        if (!preferred) entries.value = [];
      }
    } catch (error) {
      groupsError.value = String(error);
    } finally {
      loadingGroups.value = false;
    }
  }

  async function loadEntries() {
    if (!selectedGroup.value) {
      entries.value = [];
      return;
    }
    loadingEntries.value = true;
    entriesError.value = "";
    try {
      const res = await fetch(
        `/api/system/permission-groups/${encodeURIComponent(selectedGroup.value)}/entries`,
      );
      if (!res.ok) {
        entriesError.value = await errorText(res);
        entries.value = [];
        return;
      }
      entries.value = await res.json();
    } catch (error) {
      entriesError.value = String(error);
    } finally {
      loadingEntries.value = false;
    }
  }

  function openCreateGroup() {
    newGroupName.value = "";
    groupDialogError.value = "";
    groupDialogVisible.value = true;
  }

  async function createGroup() {
    const name = newGroupName.value.trim();
    if (!name) {
      groupDialogError.value = "请输入权限组名";
      return;
    }
    savingGroup.value = true;
    groupDialogError.value = "";
    try {
      const res = await fetch("/api/system/permission-groups", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name }),
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        groupDialogError.value = data.error ?? (await errorText(res));
        return;
      }
      groupDialogVisible.value = false;
      await loadGroups();
      selectedGroup.value = name;
    } catch (error) {
      groupDialogError.value = String(error);
    } finally {
      savingGroup.value = false;
    }
  }

  function openCreateEntry() {
    editingEntry.value = null;
    entryForm.value = emptyEntryForm();
    entryDialogError.value = "";
    entryDialogVisible.value = true;
  }

  function openEditEntry(entry: PermissionEntry) {
    editingEntry.value = entry;
    entryForm.value = {
      entity_id: entry.entity_id,
      entity_type: entry.entity_type,
      category: entry.category,
      value: entry.value,
    };
    entryDialogError.value = "";
    entryDialogVisible.value = true;
  }

  async function saveEntry() {
    const form = entryForm.value;
    const entityId = form.entity_id.trim();
    const category = form.category.trim();
    const validation = validateEntry(entityId, category);
    if (validation) {
      entryDialogError.value = validation;
      return;
    }

    savingEntry.value = true;
    entryDialogError.value = "";
    try {
      if (editingEntry.value) {
        const res = await fetch(
          `/api/system/permission-groups/${encodeURIComponent(selectedGroup.value)}/entries`,
          {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              entity_id: entityId,
              entity_type: form.entity_type,
              category,
              value: form.value,
            }),
          },
        );
        if (!res.ok) {
          entryDialogError.value = await errorText(res);
          return;
        }
        entryDialogVisible.value = false;
        await loadEntries();
        await refreshGroupCounts();
        return;
      }

      const res = await fetch(
        `/api/system/permission-groups/${encodeURIComponent(selectedGroup.value)}/entries`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            entity_id: entityId,
            entity_type: form.entity_type,
            category,
            value: form.value,
          }),
        },
      );
      if (res.status === 409) {
        const data = await res.json().catch(() => ({}));
        const existing = data.entry as PermissionEntry | undefined;
        if (existing) {
          pendingOverwrite.value = { entity_id: entityId, entity_type: form.entity_type, category, value: form.value };
          conflictEntry.value = existing;
          conflictMessage.value = `条目（${existing.entity_id} / ${existing.entity_type} / ${existing.category}）已存在。创建时间：${formatTime(existing.created_at)}，最后修改时间：${formatTime(existing.updated_at)}。`;
          entryDialogVisible.value = false;
          conflictVisible.value = true;
          return;
        }
        entryDialogError.value = data.error ?? "条目已存在";
        return;
      }
      if (!res.ok) {
        entryDialogError.value = await errorText(res);
        return;
      }
      entryDialogVisible.value = false;
      await loadEntries();
      await refreshGroupCounts();
    } catch (error) {
      entryDialogError.value = String(error);
    } finally {
      savingEntry.value = false;
    }
  }

  async function overwriteEntry() {
    const form = pendingOverwrite.value;
    if (!form) return;
    overwriting.value = true;
    try {
      const res = await fetch(
        `/api/system/permission-groups/${encodeURIComponent(selectedGroup.value)}/entries`,
        {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(form),
        },
      );
      if (!res.ok) {
        conflictMessage.value = await errorText(res);
        return;
      }
      conflictVisible.value = false;
      pendingOverwrite.value = null;
      conflictEntry.value = null;
      await loadEntries();
      await refreshGroupCounts();
    } catch (error) {
      conflictMessage.value = String(error);
    } finally {
      overwriting.value = false;
    }
  }

  function cancelConflict() {
    conflictVisible.value = false;
    pendingOverwrite.value = null;
    conflictEntry.value = null;
  }

  async function deleteEntry(entry: PermissionEntry) {
    entriesError.value = "";
    try {
      const params = new URLSearchParams({
        entity_id: entry.entity_id,
        entity_type: entry.entity_type,
        category: entry.category,
      });
      const res = await fetch(
        `/api/system/permission-groups/${encodeURIComponent(selectedGroup.value)}/entries?${params.toString()}`,
        { method: "DELETE" },
      );
      if (!res.ok) {
        entriesError.value = await errorText(res);
        return;
      }
      await loadEntries();
      await refreshGroupCounts();
    } catch (error) {
      entriesError.value = String(error);
    }
  }

  async function refreshGroupCounts() {
    try {
      const res = await fetch("/api/system/permission-groups");
      if (res.ok) groups.value = await res.json();
    } catch (error) {
      console.error("Failed to refresh permission groups", error);
    }
  }

  function validateEntry(entityId: string, category: string): string | null {
    if (!entityId) return "请输入实体 ID";
    if (!category) return "请输入权限类别";
    const segments = category.split(".");
    if (segments.some((segment) => !segment.trim())) return "权限类别不能包含空段";
    if (segments.slice(0, -1).some((segment) => segment === "*")) {
      return "通配符 * 只能用于最后一段，例如 A.B.*";
    }
    return null;
  }

  async function errorText(res: Response): Promise<string> {
    try {
      const data = await res.json();
      return data.error ?? `${res.status} ${res.statusText}`;
    } catch {
      return `${res.status} ${res.statusText}`;
    }
  }

  function entryRowKey(row: PermissionEntry): string {
    return `${row.entity_type}:${row.entity_id}:${row.category}`;
  }

  function entityTypeLabel(value: string): string {
    return value === "wxid" ? "wxid（微信）" : "qq_id（QQ）";
  }

  function formatTime(value: string): string {
    if (!value) return "-";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  return {
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
    groupColumns,
    entryColumns,
    loadGroups,
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
  };
}

export type UsePermissionsReturn = ReturnType<typeof usePermissions>;
