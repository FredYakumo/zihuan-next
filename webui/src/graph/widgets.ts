// Node widget setup — adds inline value widgets and special editor buttons to LiteGraph nodes

import type { NodeDefinition } from "../api/types";
import { graphs } from "../api/client";
import {
  openFormatStringEditor,
  openJsonExtractEditor,
  openFunctionSignatureEditor,
  openBrainToolsEditor,
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
    } else if (dt === "Integer" || dt === "Float") {
      lNode.addWidget("number", key, existingValue ?? 0, async (val: number) => {
        const sid = getSessionId();
        if (!sid) return;
        try {
          await graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        } catch (e) { console.error("widget update failed", e); }
      });
      addedWidget = true;
    } else if (dt === "String" || dt === "Password") {
      lNode.addWidget("text", key, String(existingValue ?? ""), async (val: string) => {
        const sid = getSessionId();
        if (!sid) return;
        try {
          await graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: val } });
        } catch (e) { console.error("widget update failed", e); }
      });
      addedWidget = true;
    }
    // Link widget to its input slot so LiteGraph collapses the double row
    // and automatically greys out the widget when a wire is connected.
    if (addedWidget) {
      const inputIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.name === key) ?? -1;
      if (inputIdx >= 0) {
        lNode.inputs[inputIdx].widget = { name: key };
      }
    }
    // Other types (refs, etc.) don't get inline widgets
  }
}
