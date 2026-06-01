import { graphs } from "../../api/client";
import type { NodeDefinition } from "../../api/types";
import type { WidgetMutationCallback } from "./types";

export function setupStringDataWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
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
    } catch (e) {
      console.error("widget update failed", e);
    }
  });
}
