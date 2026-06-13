import type { NodeDefinition } from "../../api/types";
import {
  openBrainToolsEditor,
  type BrainToolDefinition,
} from "../../ui/dialogs/index";
import type { EnterSubgraphCallback } from "./types";

export function setupBrainWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: EnterSubgraphCallback
): void {
  const tools = (nodeDef.inline_values?.["tools_config"] as BrainToolDefinition[] | undefined) ?? [];
  const isQqChatAgent =
    nodeDef.node_type === "qq_chat_agent_service" || nodeDef.node_type === "qq_chat_agent" || nodeDef.node_type === "qq_message_agent";
  const labelPrefix = isQqChatAgent ? "管理 Agent 工具" : "管理工具";
  lNode.addWidget("button", `${labelPrefix} (${tools.length})`, null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openBrainToolsEditor(nodeDef, sid, onRefresh, (toolIndex, toolDef) => {
      onEnterSubgraph(nodeDef, "brain-tool", toolIndex, toolDef, undefined);
    });
  });
}
