import type { NodeDefinition } from "../../api/types";
import { openOpenAIMessageListEditor } from "../../ui/dialogs/index";

export function setupOpenAIMessageListWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const messages = (nodeDef.inline_values?.["messages"] as Array<{ role?: string; content?: string | null }> | undefined) ?? [];
  const preview = messages.length > 0 ? `编辑 OpenAI 消息 (${messages.length})` : "编辑 OpenAI 消息";
  lNode.addWidget("button", preview, null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openOpenAIMessageListEditor(nodeDef, sid, onRefresh);
  });
}
