import {
  fileIO,
  system,
  type ConnectionConfig,
  type LlmConfig,
} from "../../api/client";

export type ActiveBotAdapterConnection = {
  connection_id: string;
  name: string;
  ws_url: string;
};

export const CONNECTION_PLACEHOLDER_VALUE = "__zihuan_no_connection__";
export const LLM_PLACEHOLDER_VALUE = "__zihuan_no_llm__";

export const AGENT_LLM_KIND_OPTIONS = [
  { value: "main", label: "主模型" },
  { value: "math_programming", label: "数学编程模型" },
  { value: "natural_language_reply", label: "自然语言回复模型" },
] as const;

let cachedTextEmbeddingModels: string[] | null = null;

export async function getTextEmbeddingModels(forceRefresh = false): Promise<string[]> {
  if (forceRefresh) cachedTextEmbeddingModels = null;
  if (cachedTextEmbeddingModels) return cachedTextEmbeddingModels;
  const response = await fileIO.listTextEmbeddingModels();
  cachedTextEmbeddingModels = response.models;
  return cachedTextEmbeddingModels;
}

export function loadTextEmbeddingModelOptions(widget: any, lNode: any): void {
  getTextEmbeddingModels()
    .then((models) => {
      widget._modelOptions = models;
      if (!widget.value && models.length > 0) {
        widget.value = models[0];
        widget._selectedModelName = models[0];
      }
      widget.label = embeddingModelButtonLabel(widget._selectedModelName || widget.value || "");
      lNode?.setDirtyCanvas?.(true, true);
    })
    .catch((error) => {
      console.error("failed to load local text embedding models", error);
      widget._modelOptions = [];
      widget.label = "加载模型失败";
    });
}

export function embeddingModelButtonLabel(modelName: string): string {
  return modelName?.trim() ? modelName : "请选择模型...";
}

export async function getConnections(): Promise<ConnectionConfig[]> {
  return system.connections.list();
}

export async function getChatLlmRefs(): Promise<LlmConfig[]> {
  const llmRefs = await system.llm.list();
  return llmRefs.filter((item) => item.enabled && item.model.type === "chat_llm");
}

export function matchesConnectionKind(actualKind: string, expectedKind: string): boolean {
  if (actualKind === expectedKind) return true;
  const botAdapterKinds = new Set(["bot_adapter", "ims_bot_adapter"]);
  if (botAdapterKinds.has(actualKind) && botAdapterKinds.has(expectedKind)) {
    return true;
  }
  return false;
}

export function loadConnectionOptions(
  widget: any,
  lNode: any,
  connectionKind: string,
  initialValue: string,
): void {
  getConnections()
    .then((connections) => {
      widget._connectionValues = buildConnectionValueMap(connections, connectionKind, initialValue);
      widget._selectedConnectionId = initialValue || CONNECTION_PLACEHOLDER_VALUE;
      widget.label = connectionButtonLabel(
        connections,
        connectionKind,
        initialValue || CONNECTION_PLACEHOLDER_VALUE,
      );
      lNode?.setDirtyCanvas?.(true, true);
    })
    .catch((error) => {
      console.error("failed to load connection options", error);
      widget._connectionValues = { [CONNECTION_PLACEHOLDER_VALUE]: "加载连接失败" };
      widget._selectedConnectionId = CONNECTION_PLACEHOLDER_VALUE;
      widget.label = "加载连接失败";
    });
}

export function loadLlmRefOptions(widget: any, lNode: any, initialValue: string): void {
  getChatLlmRefs()
    .then((llmRefs) => {
      widget._llmRefValues = buildLlmRefValueMap(llmRefs, initialValue);
      widget._selectedLlmRefId = initialValue || LLM_PLACEHOLDER_VALUE;
      widget.label = llmRefButtonLabel(llmRefs, initialValue || LLM_PLACEHOLDER_VALUE);
      lNode?.setDirtyCanvas?.(true, true);
    })
    .catch((error) => {
      console.error("failed to load llm ref options", error);
      widget._llmRefValues = { [LLM_PLACEHOLDER_VALUE]: "加载 LLM 配置失败" };
      widget._selectedLlmRefId = LLM_PLACEHOLDER_VALUE;
      widget.label = "加载 LLM 配置失败";
      lNode?.setDirtyCanvas?.(true, true);
    });
}

export function buildConnectionValueMap(
  connections: ConnectionConfig[],
  connectionKind: string,
  initialValue: string,
): Record<string, string> {
  const values: Record<string, string> = {
    [CONNECTION_PLACEHOLDER_VALUE]: "请选择连接...",
  };
  let hasCurrentValue = !initialValue;
  for (const connection of connections) {
    if (!connection.enabled) continue;
    if (!matchesConnectionKind(String(connection.kind.type ?? ""), connectionKind)) continue;
    values[connection.config_id] = connection.name;
    if (connection.config_id === initialValue) hasCurrentValue = true;
  }
  if (initialValue && !hasCurrentValue) {
    values[initialValue] = `(失效) ${initialValue}`;
  }
  return values;
}

export function agentLlmKindButtonLabel(selectedKind: string): string {
  return AGENT_LLM_KIND_OPTIONS.find((item) => item.value === selectedKind)?.label ?? "主模型";
}

export function connectionButtonLabel(
  connections: ConnectionConfig[],
  connectionKind: string,
  selectedId: string,
): string {
  if (!selectedId || selectedId === CONNECTION_PLACEHOLDER_VALUE) {
    return "请选择连接...";
  }
  const matched = connections.find((item) => (
    item.enabled
      && matchesConnectionKind(String(item.kind.type ?? ""), connectionKind)
      && item.config_id === selectedId
  ));
  return matched ? `${matched.name} (${matched.config_id})` : `(失效) ${selectedId}`;
}

export function buildLlmRefValueMap(
  llmRefs: LlmConfig[],
  initialValue: string,
): Record<string, string> {
  const values: Record<string, string> = {
    [LLM_PLACEHOLDER_VALUE]: "请选择 LLM 配置...",
  };
  let hasCurrentValue = !initialValue;
  for (const llmRef of llmRefs) {
    values[llmRef.config_id] = llmRef.name;
    if (llmRef.config_id === initialValue) hasCurrentValue = true;
  }
  if (initialValue && !hasCurrentValue) {
    values[initialValue] = `(失效) ${initialValue}`;
  }
  return values;
}

export function llmRefButtonLabel(
  llmRefs: LlmConfig[],
  selectedId: string,
): string {
  if (!selectedId || selectedId === LLM_PLACEHOLDER_VALUE) {
    return "请选择 LLM 配置...";
  }
  const matched = llmRefs.find((item) => item.config_id === selectedId);
  return matched ? `${matched.name} (${matched.config_id})` : `(失效) ${selectedId}`;
}

export function loadActiveBotAdapterOptions(
  widget: any,
  lNode: any,
  initialValue: string,
): void {
  system.connections
    .listActiveBotAdapters()
    .then((connections) => {
      const values: Record<string, string> = {
        [CONNECTION_PLACEHOLDER_VALUE]: "请选择连接...",
      };
      let hasCurrentValue = !initialValue;
      for (const connection of connections) {
        values[connection.connection_id] = connection.name;
        if (connection.connection_id === initialValue) {
          hasCurrentValue = true;
        }
      }
      if (initialValue && !hasCurrentValue) {
        values[initialValue] = `(未激活) ${initialValue}`;
      }
      widget._activeBotAdapterValues = values;
      widget._selectedConnectionId = initialValue || CONNECTION_PLACEHOLDER_VALUE;
      widget.label = activeBotAdapterButtonLabel(
        connections,
        initialValue || CONNECTION_PLACEHOLDER_VALUE,
      );
      lNode?.setDirtyCanvas?.(true, true);
    })
    .catch((error) => {
      console.error("failed to load active bot adapter options", error);
      widget._activeBotAdapterValues = { [CONNECTION_PLACEHOLDER_VALUE]: "加载连接失败" };
      widget._selectedConnectionId = CONNECTION_PLACEHOLDER_VALUE;
      widget.label = "加载连接失败";
      lNode?.setDirtyCanvas?.(true, true);
    });
}

export function activeBotAdapterButtonLabel(
  connections: ActiveBotAdapterConnection[],
  selectedId: string,
): string {
  if (!selectedId || selectedId === CONNECTION_PLACEHOLDER_VALUE) {
    return "请选择连接...";
  }
  const matched = connections.find((item) => item.connection_id === selectedId);
  return matched ? `${matched.name} (${matched.ws_url})` : `(未激活) ${selectedId}`;
}
