import { graphs } from "../../api/client";
import type { NodeDefinition } from "../../api/types";
import { getInlineWidgetTopY } from "../inline_layout";
import { portTypeString } from "../registry";
import type { WidgetMutationCallback } from "../node_widgets/types";
import { attachWidgetClickProxy } from "./click_proxy";
import { showTextEmbeddingModelPicker } from "./select_dialogs";
import {
  embeddingModelButtonLabel,
  getTextEmbeddingModels,
  loadTextEmbeddingModelOptions,
} from "./select_options";

export function setupSimpleInlineWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
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
        } catch (e) {
          console.error("widget update failed", e);
        }
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
        } catch (e) {
          console.error("widget update failed", e);
        }
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
        } catch (e) {
          console.error("widget update failed", e);
        }
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
        } catch (e) {
          console.error("widget update failed", e);
        }
      });
      if (dt === "Password" && addedWidget) (addedWidget as any)._isPassword = true;
    }

    if (addedWidget) {
      linkInlineWidgetToInput(lNode, key, addedWidget);
    }
  }

  alignInlineWidgetStack(lNode, nodeDef);
}

function linkInlineWidgetToInput(lNode: any, key: string, addedWidget: any): void {
  const inputIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.name === key) ?? -1;
  if (inputIdx < 0) return;

  lNode.inputs[inputIdx].widget = { name: key };
  lNode.inputs[inputIdx].label = "";
  addedWidget.y = getInlineWidgetTopY(lNode, inputIdx);
  addedWidget._inlineInputIndex = inputIdx;
}

function alignInlineWidgetStack(lNode: any, nodeDef: NodeDefinition): void {
  const firstLinkedIdx = (lNode.inputs as any[])?.findIndex((inp: any) => inp.widget) ?? -1;
  if (firstLinkedIdx < 0) return;

  lNode.widgets_start_y = getInlineWidgetTopY(lNode, firstLinkedIdx);
  lNode._hasInlineWidgets = true;
  if (!nodeDef.size) {
    lNode.size = lNode.computeSize();
    const INLINE_MIN_W = 130;
    if (lNode.size[0] < INLINE_MIN_W) lNode.size[0] = INLINE_MIN_W;
  }
}

function setupLocalTextEmbeddingModelWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("button", "请选择模型...", "", () => {
    void (async () => {
      const sid = getSessionId();
      if (!sid) return;
      if (nodeDef.port_bindings?.[key]) return;
      const models = await getTextEmbeddingModels();
      const selected = await showTextEmbeddingModelPicker(
        models,
        widget._selectedModelName || initialValue || "",
      );
      if (selected == null) return;
      widget.value = selected;
      widget._selectedModelName = selected;
      widget.label = embeddingModelButtonLabel(selected);
      widget._zihuanTouched = true;
      const pending = graphs.updateNode(sid, nodeDef.id, { inline_values: { [key]: selected } });
      onMutated?.(pending);
      await pending;
    })().catch((error) => {
      console.error("embedding model picker failed", error);
    });
  });
  widget.value = initialValue;
  widget._selectedModelName = initialValue || "";
  attachWidgetClickProxy(lNode, widget, () => {
    void widget.callback?.();
  });
  loadTextEmbeddingModelOptions(widget, lNode);
  return widget;
}
