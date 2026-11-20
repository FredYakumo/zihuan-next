import { onMounted, ref } from "vue";

interface CommandDef {
  name: string;
  aliases: string[];
  description: string;
  scope: string | { Specific?: { agent_ids: string[] } };
}

export function useCommands() {
  const commands = ref<CommandDef[]>([]);

  onMounted(() => {
    loadData();
  });

  async function loadData() {
    try {
      const res = await fetch("/api/system/commands/registry");
      if (res.ok) {
        commands.value = await res.json();
      }
    } catch (e) {
      console.error("Failed to load commands registry", e);
    }
  }

  const commandColumns = [
    { colKey: "name", title: "命令名" },
    { colKey: "aliases", title: "别名" },
    { colKey: "accepted_arg_count", title: "参数数" },
    { colKey: "description", title: "描述" },
    { colKey: "scope", title: "适用范围" },
  ];

  function scopeLabel(scope: unknown): string {
    if (typeof scope === "string") {
      const map: Record<string, string> = {
        all: "全部",
        qq_chat: "QQ Chat",
        workspace: "Workspace",
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

  return {
    commands,
    loadData,
    commandColumns,
    scopeLabel,
  };
}

export type UseCommandsReturn = ReturnType<typeof useCommands>;
