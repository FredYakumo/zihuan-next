import type { NodeDefinition } from "../../api/types";
import { openJsonExtractEditor } from "../../ui/dialogs/index";

export function setupJsonExtractWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  const fields = (nodeDef.inline_values?.["fields_config"] as Array<{ name: string }> | undefined) ?? [];
  lNode.addWidget("button", `配置字段 (${fields.length})`, null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openJsonExtractEditor(nodeDef, sid, onRefresh);
  });
}
