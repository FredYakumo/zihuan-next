// Node widget setup — adds inline value widgets and special editor buttons to LiteGraph nodes

import type { NodeDefinition } from "../api/types";
import { graphs } from "../api/client";
import { LiteGraph } from "litegraph.js";
import {
  openFormatStringEditor,
  openJsonExtractEditor,
  openFunctionSignatureEditor,
  openBrainToolsEditor,
  openQQMessageListEditor,
  type BrainToolDefinition,
  type EmbeddedFunctionConfig,
} from "../ui/dialogs";

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
  ) => void
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
    case "string_data":
      setupStringDataWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "qq_message_list_data":
      setupQQMessageListWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    default:
      setupSimpleInlineWidgets(lNode, nodeDef, getSessionId, onRefresh);
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
  lNode.addWidget("button", "编辑签名/子图", null, () => {
    const sid = getSessionId();
    if (!sid) { alert("请先打开一个图。"); return; }
    openFunctionSignatureEditor(
      nodeDef, sid, onRefresh,
      (config) => onEnterSubgraph(nodeDef, "function", undefined, undefined, config)
    );
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
  lNode.addWidget("button", `管理工具 (${tools.length})`, null, () => {
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
  onRefresh: () => void
): void {
  const currentValue = String(nodeDef.inline_values?.["text"] ?? "");
  lNode.addWidget("text", "text", currentValue, async (val: string) => {
    const sid = getSessionId();
    if (!sid) return;
    try {
      await graphs.updateNode(sid, nodeDef.id, { inline_values: { text: val } });
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
  onRefresh: () => void
): void {
  for (const port of nodeDef.input_ports) {
    const key = port.name;
    const existingValue = nodeDef.inline_values?.[key];
    const dt = typeof port.data_type === "string" ? port.data_type : "Any";

    let addedWidget = false;
    if (dt === "Boolean") {
      lNode.addWidget("toggle", key, existingValue ?? false, async (val: boolean) => {
        const sid = getSessionId();
        if (!sid) return;
        try {
          await graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        } catch (e) { console.error("widget update failed", e); }
      });
      addedWidget = true;
    } else if (dt === "Integer") {
      lNode.addWidget("number", key, existingValue ?? 0, async (val: number) => {
        const sid = getSessionId();
        if (!sid) return;
        try {
          await graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: Math.trunc(val) } });
        } catch (e) { console.error("widget update failed", e); }
      }, { precision: 0, step: 10 });
      addedWidget = true;
    } else if (dt === "Float") {
      lNode.addWidget("number", key, existingValue ?? 0, async (val: number) => {
        const sid = getSessionId();
        if (!sid) return;
        try {
          await graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        } catch (e) { console.error("widget update failed", e); }
      });
      addedWidget = true;
    } else if (dt === "String" || dt === "Password") {
      const w = lNode.addWidget("text", key, String(existingValue ?? ""), async (val: string) => {
        const sid = getSessionId();
        if (!sid) return;
        try {
          await graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        } catch (e) { console.error("widget update failed", e); }
      });
      if (dt === "Password" && w) (w as any)._isPassword = true;
      addedWidget = true;
    }
    // Link widget to its input slot for right-click binding and badge rendering.
    // Suppress the duplicate slot label so only the widget row is visible;
    // widgets_start_y (set below) moves the widget up to the same row as the dot.
    if (addedWidget) {
      const inputIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.name === key) ?? -1;
      if (inputIdx >= 0) {
        lNode.inputs[inputIdx].widget = { name: key };
        // Empty label → LiteGraph skips drawing the slot name text, removing
        // the duplicate label that would otherwise appear left of the dot.
        lNode.inputs[inputIdx].label = "";
      }
    }
    // Other types (refs, etc.) don't get inline widgets
  }

  // Co-locate each widget with its linked input slot row.
  // LiteGraph v0.7.18 does NOT do this automatically — we must set widgets_start_y
  // so the first widget aligns with the first widget-linked slot row.
  const SLOT_H: number = (LiteGraph as any).NODE_SLOT_HEIGHT ?? 20;
  const WIDGET_H: number = (LiteGraph as any).NODE_WIDGET_HEIGHT ?? 20;
  const slotStartY: number = (lNode.constructor as any).slot_start_y ?? 0;
  const firstLinkedIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.widget) ?? -1;
  if (firstLinkedIdx >= 0) {
    // Center the first widget on the same y as the first widget-linked slot.
    const slotCenterY = slotStartY + (firstLinkedIdx + 0.7) * SLOT_H;
    lNode.widgets_start_y = slotCenterY - WIDGET_H / 2 - 2;
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
