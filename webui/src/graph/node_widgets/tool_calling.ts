import type { NodeDefinition } from "../../api/types";
import {
  openToolCallingToolsEditor,
  type ToolDefinition,
} from "../../ui/dialogs/index";
import type { EnterSubgraphCallback } from "./types";

export function setupToolCallingWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: EnterSubgraphCallback
): void {
  const tools = (nodeDef.inline_values?.["tools_config"] as ToolDefinition[] | undefined) ?? [];
  const isQqChatAgent = nodeDef.node_type === "qq_chat";
  const labelPrefix = isQqChatAgent ? "管理 Agent 工具" : "管理工具";
  lNode.addWidget("button", `${labelPrefix} (${tools.length})`, null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openToolCallingToolsEditor(nodeDef, sid, onRefresh, (toolIndex, toolDef) => {
      onEnterSubgraph(nodeDef, "tool-calling-tool", toolIndex, toolDef, undefined);
    });
  });
}
