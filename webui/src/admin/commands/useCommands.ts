import { onMounted, ref } from "vue";

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


export function useCommands() {
  const commands = ref<CommandDef[]>([]);
  const permissions = ref<CommandPermission[]>([]);
  const showEditor = ref(false);
  const editingId = ref<string | null>(null);

  const form = ref<PermissionForm>({
    command_name: "",
    enabled: true,
    rules: [],
  });

  onMounted(() => {
    loadData();
  });

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

  function permissionFor(commandName: string): CommandPermission | undefined {
    return permissions.value.find((p) => p.command_name === commandName);
  }

  function scopeLabel(scope: unknown): string {
    if (typeof scope === "string") {
      const map: Record<string, string> = {
        all: "全部",
        qq_chat: "QQ Chat",
        http_stream: "HTTP Stream",
      };
      return map[scope] ?? scope;
    }
    if (scope && typeof scope === "object" && "Specific" in scope) {
      const specific = (scope as { Specific?: { agent_ids?: string[] } }).Specific;
      return `指定 Agent（${specific?.agent_ids?.join(", ") ?? ""}）`;
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
    void rule;
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

  return {
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
  };
}

export type UseCommandsReturn = ReturnType<typeof useCommands>;
