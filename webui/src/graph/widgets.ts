// Node widget setup — adds inline value widgets and special editor buttons to LiteGraph nodes

import type { NodeDefinition } from "../api/types";
import { fileIO, graphs, system, type ConnectionConfig } from "../api/client";
import { getNodeTypeInfo, portTypeString } from "./registry";
import {
  openFormatStringEditor,
  openJsonExtractEditor,
  openFunctionSignatureEditor,
  openBrainToolsEditor,
  openOpenAIMessageListEditor,
  openQQMessageListEditor,
  type BrainToolDefinition,
  type EmbeddedFunctionConfig,
} from "../ui/dialogs/index";
import { ensureDialogStyles, openOverlay } from "../ui/dialogs/base";
import { getInlineWidgetTopY } from "./inline_layout";
import { NODE_TITLE_HEIGHT } from "./canvas/rendering";
import { setupQQMessagePreviewWidgets } from "./preview_qq_messages";

type WidgetMutationCallback = (pending?: Promise<unknown>) => void;

let cachedTextEmbeddingModels: string[] | null = null;
const CONNECTION_PLACEHOLDER_VALUE = "__zihuan_no_connection__";

/** Called for every node added to the canvas after the node is created. */
export function setupNodeWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: (
    parentNodeDef: NodeDefinition,
    mode: "function" | "brain-tool",
    toolIndex?: number,
    toolDef?: BrainToolDefinition,
    functionConfig?: EmbeddedFunctionConfig
  ) => void,
  onMutated?: WidgetMutationCallback
): void {
  const typeId = nodeDef.node_type;

  switch (typeId) {
    case "format_string":
      setupFormatStringWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "json_extract":
      setupJsonExtractWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "function":
      setupFunctionWidgets(lNode, nodeDef, getSessionId, onRefresh, onEnterSubgraph);
      break;
    case "brain":
      setupBrainWidgets(lNode, nodeDef, getSessionId, onRefresh, onEnterSubgraph);
      break;
    case "qq_chat_agent":
    case "qq_message_agent":
      setupBrainWidgets(lNode, nodeDef, getSessionId, onRefresh, onEnterSubgraph);
      break;
    case "string_data":
      setupStringDataWidgets(lNode, nodeDef, getSessionId, onRefresh, onMutated);
      break;
    case "message_list_data":
      setupOpenAIMessageListWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "qq_message_list_data":
      setupQQMessageListWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "qq_message_preview":
      setupQQMessagePreviewWidgets(lNode, nodeDef);
      break;
    default:
      setupSimpleInlineWidgets(lNode, nodeDef, getSessionId, onRefresh, onMutated);
      break;
  }
  setupConfigFieldWidgets(lNode, nodeDef, getSessionId, onMutated);
}

// ─── Format String ────────────────────────────────────────────────────────────

function setupFormatStringWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const template = (nodeDef.inline_values?.["template"] as string) ?? "";
  const preview = template.length > 40 ? template.slice(0, 40) + "…" : template || "(空模板)";

  lNode.addWidget("button", `✏ ${preview}`, null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openFormatStringEditor(nodeDef, sid, onRefresh);
  });
}

// ─── JSON Extract ─────────────────────────────────────────────────────────────

function setupJsonExtractWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const fields = (nodeDef.inline_values?.["fields_config"] as Array<{ name: string }> | undefined) ?? [];
  lNode.addWidget("button", `配置字段 (${fields.length})`, null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openJsonExtractEditor(nodeDef, sid, onRefresh);
  });
}

// ─── Function ─────────────────────────────────────────────────────────────────

function setupFunctionWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: (
    parentNodeDef: NodeDefinition,
    mode: "function" | "brain-tool",
    toolIndex?: number,
    toolDef?: BrainToolDefinition,
    functionConfig?: EmbeddedFunctionConfig
  ) => void
): void {
  const currentFunctionConfig = (): EmbeddedFunctionConfig => ({
    name: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.name) ?? nodeDef.name,
    description: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.description) ?? "",
    inputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.inputs) ?? [],
    outputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.outputs) ?? [],
    subgraph: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.subgraph) ?? {
      nodes: [], edges: [], graph_inputs: [], graph_outputs: [], hyperparameter_groups: [], hyperparameters: [], variables: [],
    } as any,
  });

  lNode.addWidget("button", "编辑签名/子图", null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openFunctionSignatureEditor(
      nodeDef, sid, onRefresh,
      (config) => onEnterSubgraph(nodeDef, "function", undefined, undefined, config)
    );
  });

  lNode.addWidget("button", "↳ 进入子图", null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    onEnterSubgraph(nodeDef, "function", undefined, undefined, currentFunctionConfig());
  });
}

// ─── Brain ────────────────────────────────────────────────────────────────────

function setupBrainWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: (
    parentNodeDef: NodeDefinition,
    mode: "function" | "brain-tool",
    toolIndex?: number,
    toolDef?: BrainToolDefinition,
    functionConfig?: EmbeddedFunctionConfig
  ) => void
): void {
  const tools = (nodeDef.inline_values?.["tools_config"] as BrainToolDefinition[] | undefined) ?? [];
  const isQqChatAgent =
    nodeDef.node_type === "qq_chat_agent" || nodeDef.node_type === "qq_message_agent";
  const labelPrefix = isQqChatAgent ? "管理 Agent 工具" : "管理工具";
  lNode.addWidget("button", `${labelPrefix} (${tools.length})`, null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openBrainToolsEditor(nodeDef, sid, onRefresh, (toolIndex, toolDef) => {
      onEnterSubgraph(nodeDef, "brain-tool", toolIndex, toolDef, undefined);
    });
  });
}

// ─── String Data ──────────────────────────────────────────────────────────────

function setupStringDataWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onMutated?: WidgetMutationCallback
): void {
  const currentValue = String(nodeDef.inline_values?.["text"] ?? "");
  const widget = lNode.addWidget("text", "text", currentValue, async (val: string) => {
    const sid = getSessionId();
    if (!sid) return;
    widget._zihuanTouched = true;
    const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { text: val } });
    onMutated?.(pending);
    try {
      await pending;
    } catch (e) { console.error("widget update failed", e); }
  });
}

// ─── QQMessage List Data ──────────────────────────────────────────────────────

function setupOpenAIMessageListWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const messages = (nodeDef.inline_values?.["messages"] as Array<{ role?: string; content?: string | null }> | undefined) ?? [];
  const preview = messages.length > 0 ? `编辑 OpenAI 消息 (${messages.length})` : "编辑 OpenAI 消息";
  lNode.addWidget("button", preview, null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openOpenAIMessageListEditor(nodeDef, sid, onRefresh);
  });
}

// ─── QQMessage List Data ──────────────────────────────────────────────────────

function setupQQMessageListWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const messages = (nodeDef.inline_values?.["messages"] as Array<{ type: string; data: unknown }> | undefined) ?? [];
  const preview = messages.length > 0 ? `编辑消息列表 (${messages.length})` : "编辑消息列表";
  lNode.addWidget("button", preview, null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openQQMessageListEditor(nodeDef, sid, onRefresh);
  });
}

// ─── Simple inline value widgets (text / number / toggle) ─────────────────────

function setupSimpleInlineWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onMutated?: WidgetMutationCallback
): void {
  for (const port of nodeDef.input_ports) {
    const key = port.name;
    const existingValue = nodeDef.inline_values?.[key];
    const dt = portTypeString(port.data_type);

    let addedWidget: any | null = null;
    if (nodeDef.node_type === "load_local_text_embedder" && key === "model_name") {
      addedWidget = setupLocalTextEmbeddingModelWidget(
        lNode,
        nodeDef,
        key,
        String(existingValue ?? ""),
        getSessionId,
        onMutated,
      );
    } else if (dt === "Boolean") {
      addedWidget = lNode.addWidget("toggle", key, existingValue ?? false, async (val: boolean) => {
        const sid = getSessionId();
        if (!sid) return;
        if (nodeDef.port_bindings?.[key]) return;
        addedWidget!._zihuanTouched = true;
        const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        onMutated?.(pending);
        try {
          await pending;
        } catch (e) { console.error("widget update failed", e); }
      });
    } else if (dt === "Integer") {
      addedWidget = lNode.addWidget("number", key, existingValue ?? 0, async (val: number) => {
        const sid = getSessionId();
        if (!sid) return;
        if (nodeDef.port_bindings?.[key]) return;
        addedWidget!._zihuanTouched = true;
        const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: Math.trunc(val) } });
        onMutated?.(pending);
        try {
          await pending;
        } catch (e) { console.error("widget update failed", e); }
      }, { precision: 0, step: 10 });
    } else if (dt === "Float") {
      addedWidget = lNode.addWidget("number", key, existingValue ?? 0, async (val: number) => {
        const sid = getSessionId();
        if (!sid) return;
        if (nodeDef.port_bindings?.[key]) return;
        addedWidget!._zihuanTouched = true;
        const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        onMutated?.(pending);
        try {
          await pending;
        } catch (e) { console.error("widget update failed", e); }
      });
    } else if (dt === "String" || dt === "Password") {
      addedWidget = lNode.addWidget("text", key, String(existingValue ?? ""), async (val: string) => {
        const sid = getSessionId();
        if (!sid) return;
        if (nodeDef.port_bindings?.[key]) return;
        addedWidget!._zihuanTouched = true;
        const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        onMutated?.(pending);
        try {
          await pending;
        } catch (e) { console.error("widget update failed", e); }
      });
      if (dt === "Password" && addedWidget) (addedWidget as any)._isPassword = true;
    }
    // Link widget to its input slot for right-click binding and badge rendering.
    // Suppress the duplicate slot label so only the widget row is visible;
    // widget.y (set below) pins every inline widget to its corresponding slot row,
    // avoiding LiteGraph's default +4px per-widget drift between rows.
    if (addedWidget) {
      const inputIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.name === key) ?? -1;
      if (inputIdx >= 0) {
        lNode.inputs[inputIdx].widget = { name: key };
        // Empty label → LiteGraph skips drawing the slot name text, removing
        // the duplicate label that would otherwise appear left of the dot.
        lNode.inputs[inputIdx].label = "";
        addedWidget.y = getInlineWidgetTopY(lNode, inputIdx);
        addedWidget._inlineInputIndex = inputIdx;
      }
    }
    // Other types (refs, etc.) don't get inline widgets
  }

  // Co-locate the widget stack with its first linked input slot row.
  // Individual widget.y values pin every inline widget to its own slot row,
  // while widgets_start_y keeps LiteGraph's auto-size and first-row origin sane.
  const firstLinkedIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.widget) ?? -1;
  if (firstLinkedIdx >= 0) {
    lNode.widgets_start_y = getInlineWidgetTopY(lNode, firstLinkedIdx);
    // Mark this node so drawInlineOutputLabels knows to re-draw output labels
    // on top of the widget backgrounds that would otherwise cover them.
    lNode._hasInlineWidgets = true;
    // Recompute node height only when no explicit size is saved (new node).
    if (!nodeDef.size) {
      lNode.size = lNode.computeSize();
      // Enforce minimum width for inline layout: input dot zone (30) + content (60) + output zone (40).
      const INLINE_MIN_W = 130;
      if (lNode.size[0] < INLINE_MIN_W) lNode.size[0] = INLINE_MIN_W;
    }
  }
}

function setupConfigFieldWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): void {
  const typeInfo = getNodeTypeInfo(nodeDef.node_type);
  const configFields = typeInfo?.config_fields ?? [];
  let addedConfigWidget = false;
  for (const field of configFields) {
    if (field.widget === "active_bot_adapter_select") {
      setupActiveBotAdapterSelectWidget(
        lNode,
        nodeDef,
        field.key,
        String(nodeDef.inline_values?.[field.key] ?? ""),
        getSessionId,
        onMutated,
      );
      addedConfigWidget = true;
      continue;
    }
    if (field.widget !== "connection_select") continue;
    setupConnectionSelectWidget(
      lNode,
      nodeDef,
      field.key,
      field.connection_kind ?? "",
      String(nodeDef.inline_values?.[field.key] ?? ""),
      getSessionId,
      onMutated,
    );
    addedConfigWidget = true;
  }

  if (!addedConfigWidget) return;

  if ((lNode.widgets?.length ?? 0) > 0) {
    lNode.widgets_start_y = NODE_TITLE_HEIGHT + 8;
    if (!nodeDef.size) {
      lNode.size = lNode.computeSize();
    } else {
      const computed = lNode.computeSize();
      lNode.size = [
        Math.max(nodeDef.size.width, computed[0]),
        Math.max(nodeDef.size.height, computed[1]),
      ];
    }
  }
}

function setupActiveBotAdapterSelectWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("button", "请选择连接...", "", () => {
    void (async () => {
      const sid = getSessionId();
      if (!sid) return;
      const connections = await system.connections.listActiveBotAdapters();
      const selected = await showActiveBotAdapterPicker(
        connections,
        initialValue || widget._selectedConnectionId || "",
      );
      if (selected == null) return;
      widget._selectedConnectionId = selected;
      widget.label = activeBotAdapterButtonLabel(connections, selected);
      widget._zihuanTouched = true;
      widget._zihuanInlineKey = key;
      const pending = graphs.updateNode(sid, nodeDef.id, {
        inline_values: {
          [key]: selected && selected !== CONNECTION_PLACEHOLDER_VALUE ? selected : null,
        },
      });
      onMutated?.(pending);
      await pending;
    })();
  });
  widget.label = "加载中...";
  widget._selectedConnectionId = initialValue || CONNECTION_PLACEHOLDER_VALUE;
  widget._zihuanInlineKey = key;
  attachWidgetClickProxy(lNode, widget, () => {
    void widget.callback?.();
  });
  loadActiveBotAdapterOptions(widget, lNode, initialValue);
  return widget;
}

function setupLocalTextEmbeddingModelWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("combo", key, initialValue, async (selected: string) => {
      if (selected == null) return;
      const sid = getSessionId();
      if (!sid) return;
      if (nodeDef.port_bindings?.[key]) return;
      widget.value = selected;
      widget._zihuanTouched = true;
      const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: selected } });
      onMutated?.(pending);
      await pending;
  }, { values: [] as string[] });
  widget.value = initialValue;
  loadTextEmbeddingModelOptions(widget, lNode);
  return widget;
}

function setupConnectionSelectWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  connectionKind: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("combo", key, initialValue || CONNECTION_PLACEHOLDER_VALUE, async (selected: string) => {
    const sid = getSessionId();
    if (!sid) return;
    widget.value = selected ?? CONNECTION_PLACEHOLDER_VALUE;
    widget._zihuanTouched = true;
    widget._zihuanInlineKey = key;
    const pending = graphs.updateNode(sid, nodeDef.id, {
      inline_values: {
        [key]: selected && selected !== CONNECTION_PLACEHOLDER_VALUE ? selected : null,
      },
    });
    onMutated?.(pending);
    await pending;
  }, { values: { [CONNECTION_PLACEHOLDER_VALUE]: "请选择连接..." } as Record<string, string> });
  widget.value = initialValue || CONNECTION_PLACEHOLDER_VALUE;
  widget._zihuanInlineKey = key;
  loadConnectionOptions(widget, lNode, connectionKind, initialValue);
  return widget;
}

async function getTextEmbeddingModels(forceRefresh = false): Promise<string[]> {
  if (forceRefresh) cachedTextEmbeddingModels = null;
  if (cachedTextEmbeddingModels) return cachedTextEmbeddingModels;
  const response = await fileIO.listTextEmbeddingModels();
  cachedTextEmbeddingModels = response.models;
  return cachedTextEmbeddingModels;
}

function loadTextEmbeddingModelOptions(widget: any, lNode: any): void {
  getTextEmbeddingModels()
    .then((models) => {
      widget.options = widget.options ?? {};
      widget.options.values = models;
      if (!widget.value && models.length > 0) {
        widget.value = models[0];
      }
      lNode?.setDirtyCanvas?.(true, true);
    })
    .catch((error) => {
      console.error("failed to load local text embedding models", error);
      widget.options = widget.options ?? {};
      widget.options.values = [];
    });
}

async function getConnections(): Promise<ConnectionConfig[]> {
  return system.connections.list();
}

function matchesConnectionKind(actualKind: string, expectedKind: string): boolean {
  if (actualKind === expectedKind) return true;
  const botAdapterKinds = new Set(["bot_adapter", "ims_bot_adapter"]);
  if (botAdapterKinds.has(actualKind) && botAdapterKinds.has(expectedKind)) {
    return true;
  }
  return false;
}

function loadConnectionOptions(
  widget: any,
  lNode: any,
  connectionKind: string,
  initialValue: string,
): void {
  getConnections()
    .then((connections) => {
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
      widget.options = widget.options ?? {};
      widget.options.values = values;
      if (!widget.value || widget.value === "") {
        widget.value = CONNECTION_PLACEHOLDER_VALUE;
      }
      lNode?.setDirtyCanvas?.(true, true);
    })
    .catch((error) => {
      console.error("failed to load connection options", error);
      widget.options = widget.options ?? {};
      widget.options.values = { [CONNECTION_PLACEHOLDER_VALUE]: "加载连接失败" };
      widget.value = CONNECTION_PLACEHOLDER_VALUE;
    });
}

function loadActiveBotAdapterOptions(
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

function activeBotAdapterButtonLabel(
  connections: Array<{ connection_id: string; name: string; ws_url: string }>,
  selectedId: string,
): string {
  if (!selectedId || selectedId === CONNECTION_PLACEHOLDER_VALUE) {
    return "请选择连接...";
  }
  const matched = connections.find((item) => item.connection_id === selectedId);
  return matched ? `${matched.name} (${matched.ws_url})` : `(未激活) ${selectedId}`;
}

function attachWidgetClickProxy(
  lNode: any,
  widget: any,
  onClick: () => void,
): void {
  const previousMouseDown = lNode.onMouseDown;
  lNode.onMouseDown = (e: MouseEvent, pos: [number, number]): boolean | undefined => {
    const margin = 15;
    const widgetHeight = (window as any).LiteGraph?.NODE_WIDGET_HEIGHT ?? 20;
    const widgetTop = typeof widget.last_y === "number"
      ? widget.last_y
      : ((lNode.widgets_start_y ?? (NODE_TITLE_HEIGHT + 8)) as number);
    const widgetBottom = widgetTop + widgetHeight;
    const widgetLeft = margin;
    const widgetRight = (lNode.size?.[0] ?? 0) - margin;

    if (
      pos[0] >= widgetLeft
      && pos[0] <= widgetRight
      && pos[1] >= widgetTop
      && pos[1] <= widgetBottom
    ) {
      e.preventDefault();
      e.stopPropagation();
      onClick();
      return true;
    }

    return previousMouseDown?.(e, pos);
  };
}

async function showActiveBotAdapterPicker(
  connections: Array<{ connection_id: string; name: string; ws_url: string }>,
  selectedId: string,
): Promise<string | null> {
  ensureDialogStyles();
  const { overlay, dialog, close } = openOverlay();
  dialog.style.minWidth = "360px";
  dialog.style.maxWidth = "420px";

  const title = document.createElement("h3");
  title.textContent = "选择 IMS Bot Adapter";
  dialog.appendChild(title);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:8px;margin-top:12px;";

  const allItems = [
    { connection_id: CONNECTION_PLACEHOLDER_VALUE, name: "不选择", ws_url: "" },
    ...connections,
  ];

  if (allItems.length === 1) {
    const empty = document.createElement("div");
    empty.className = "zh-hint";
    empty.textContent = "当前没有已激活的 Bot Adapter。";
    dialog.appendChild(empty);
  } else {
    for (const item of allItems) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = item.connection_id === CONNECTION_PLACEHOLDER_VALUE
        ? item.name
        : `${item.name} (${item.ws_url})`;
      button.style.cssText = "text-align:left;";
      if (item.connection_id === selectedId) {
        button.className = "primary";
      }
      button.addEventListener("click", () => {
        close();
        resolvePromise(item.connection_id);
      });
      list.appendChild(button);
    }
    dialog.appendChild(list);
  }

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  footer.appendChild(cancelBtn);
  dialog.appendChild(footer);

  let resolved = false;
  const resolvePromise = (value: string | null) => {
    if (resolved) return;
    resolved = true;
    resolver(value);
  };
  cancelBtn.addEventListener("click", () => {
    close();
    resolvePromise(null);
  });
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      close();
      resolvePromise(null);
    }
  });

  let resolver: (value: string | null) => void = () => {};
  return new Promise<string | null>((resolve) => {
    resolver = resolve;
  });
}
