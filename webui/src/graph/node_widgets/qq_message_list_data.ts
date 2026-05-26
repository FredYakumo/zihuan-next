import type { NodeDefinition } from "../../api/types";
import { openQQMessageListEditor } from "../../ui/dialogs/index";

export function setupQQMessageListWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const messages = (nodeDef.inline_values?.["messages"] as Array<{ type: string; data: unknown }> | undefined) ?? [];
  const preview = messages.length > 0 ? `编辑消息列表 (${messages.length})` : "编辑消息列表";
  lNode.addWidget("button", preview, null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openQQMessageListEditor(nodeDef, sid, onRefresh);
  });
}
