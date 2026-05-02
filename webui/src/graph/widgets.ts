// Node widget setup — adds inline value widgets and special editor buttons to LiteGraph nodes

import type { NodeDefinition } from "../api/types";
import { graphs } from "../api/client";
import { portTypeString } from "./registry";
import {
  openFormatStringEditor,
  openJsonExtractEditor,
  openFunctionSignatureEditor,
  openBrainToolsEditor,
  openQQMessageListEditor,
  type BrainToolDefinition,
  type EmbeddedFunctionConfig,
} from "../ui/dialogs/index";
import { getInlineWidgetTopY } from "./inline_layout";

type WidgetMutationCallback = (pending?: Promise<unknown>) => void;

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
    case "qq_message_agent":
      setupBrainWidgets(lNode, nodeDef, getSessionId, onRefresh, onEnterSubgraph);
      break;
    case "string_data":
      setupStringDataWidgets(lNode, nodeDef, getSessionId, onRefresh, onMutated);
      break;
    case "qq_message_list_data":
      setupQQMessageListWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    default:
      setupSimpleInlineWidgets(lNode, nodeDef, getSessionId, onRefresh, onMutated);
      break;
  }
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
      nodes: [], edges: [], hyperparameter_groups: [], hyperparameters: [], variables: [],
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
  const labelPrefix = nodeDef.node_type === "qq_message_agent" ? "管理 Agent 工具" : "管理工具";
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
    if (dt === "Boolean") {
      addedWidget = lNode.addWidget("toggle", key, existingValue ?? false, async (val: boolean) => {
        const sid = getSessionId();
        if (!sid) return;
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
