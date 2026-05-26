import { graphs, system } from "../../api/client";
import type { NodeDefinition } from "../../api/types";
import { NODE_TITLE_HEIGHT } from "../canvas/rendering";
import { getNodeTypeInfo } from "../registry";
import type { WidgetMutationCallback } from "../node_widgets/types";
import { attachWidgetClickProxy } from "./click_proxy";
import {
  showActiveBotAdapterPicker,
  showAgentLlmKindPicker,
  showConnectionPicker,
  showLlmRefPicker,
} from "./select_dialogs";
import {
  CONNECTION_PLACEHOLDER_VALUE,
  LLM_PLACEHOLDER_VALUE,
  activeBotAdapterButtonLabel,
  agentLlmKindButtonLabel,
  connectionButtonLabel,
  getChatLlmRefs,
  getConnections,
  llmRefButtonLabel,
  loadActiveBotAdapterOptions,
  loadConnectionOptions,
  loadLlmRefOptions,
} from "./select_options";

export function setupConfigFieldWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): void {
  const typeInfo = getNodeTypeInfo(nodeDef.node_type);
  const configFields = typeInfo?.config_fields ?? [];
  let addedConfigWidget = false;
  for (const field of configFields) {
    if (field.widget === "active_bot_adapter_select") {
      setupActiveBotAdapterSelectWidget(
        lNode,
        nodeDef,
        field.key,
        String(nodeDef.inline_values?.[field.key] ?? ""),
        getSessionId,
        onMutated,
      );
      addedConfigWidget = true;
      continue;
    }
    if (field.widget === "agent_llm_kind_select") {
      setupAgentLlmKindSelectWidget(
        lNode,
        nodeDef,
        field.key,
        String(nodeDef.inline_values?.[field.key] ?? ""),
        getSessionId,
        onMutated,
      );
      addedConfigWidget = true;
      continue;
    }
    if (field.widget === "llm_ref_select") {
      setupLlmRefSelectWidget(
        lNode,
        nodeDef,
        field.key,
        String(nodeDef.inline_values?.[field.key] ?? ""),
        getSessionId,
        onMutated,
      );
      addedConfigWidget = true;
      continue;
    }
    if (field.widget !== "connection_select") continue;
    setupConnectionSelectWidget(
      lNode,
      nodeDef,
      field.key,
      field.connection_kind ?? "",
      String(nodeDef.inline_values?.[field.key] ?? ""),
      getSessionId,
      onMutated,
    );
    addedConfigWidget = true;
  }

  if (!addedConfigWidget) return;
  resizeNodeForConfigWidgets(lNode, nodeDef);
}

function resizeNodeForConfigWidgets(lNode: any, nodeDef: NodeDefinition): void {
  if ((lNode.widgets?.length ?? 0) === 0) return;

  lNode.widgets_start_y = NODE_TITLE_HEIGHT + 8;
  if (!nodeDef.size) {
    lNode.size = lNode.computeSize();
    return;
  }

  const computed = lNode.computeSize();
  lNode.size = [
    Math.max(nodeDef.size.width, computed[0]),
    Math.max(nodeDef.size.height, computed[1]),
  ];
}

function setupAgentLlmKindSelectWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("button", "请选择 LLM 类型...", "", () => {
    void (async () => {
      const sid = getSessionId();
      if (!sid) return;
      const selected = await showAgentLlmKindPicker(
        widget._selectedAgentLlmKind || initialValue || "main",
      );
      if (selected == null) return;
      widget._selectedAgentLlmKind = selected;
      widget.label = agentLlmKindButtonLabel(selected);
      widget._zihuanTouched = true;
      widget._zihuanInlineKey = key;
      const pending = graphs.updateNode(sid, nodeDef.id, {
        inline_values: {
          [key]: selected,
        },
      });
      onMutated?.(pending);
      await pending;
    })().catch((error) => {
      console.error("agent llm kind picker failed", error);
    });
  });
  widget.value = initialValue || "main";
  widget._selectedAgentLlmKind = initialValue || "main";
  widget._zihuanInlineKey = key;
  attachWidgetClickProxy(lNode, widget, () => {
    void widget.callback?.();
  });
  widget.label = agentLlmKindButtonLabel(initialValue || "main");
  return widget;
}

function setupLlmRefSelectWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("button", "请选择 LLM 配置...", "", () => {
    void (async () => {
      const sid = getSessionId();
      if (!sid) return;
      const llmRefs = await getChatLlmRefs();
      const selected = await showLlmRefPicker(
        llmRefs,
        widget._selectedLlmRefId || LLM_PLACEHOLDER_VALUE,
      );
      if (selected == null) return;
      widget._selectedLlmRefId = selected;
      widget.label = llmRefButtonLabel(llmRefs, selected);
      widget._zihuanTouched = true;
      widget._zihuanInlineKey = key;
      const pending = graphs.updateNode(sid, nodeDef.id, {
        inline_values: {
          [key]: selected && selected !== LLM_PLACEHOLDER_VALUE ? selected : null,
        },
      });
      onMutated?.(pending);
      await pending;
    })().catch((error) => {
      console.error("llm ref picker failed", error);
    });
  });
  widget.value = initialValue || LLM_PLACEHOLDER_VALUE;
  widget._selectedLlmRefId = initialValue || LLM_PLACEHOLDER_VALUE;
  widget._zihuanInlineKey = key;
  attachWidgetClickProxy(lNode, widget, () => {
    void widget.callback?.();
  });
  loadLlmRefOptions(widget, lNode, initialValue);
  return widget;
}

function setupActiveBotAdapterSelectWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("button", "请选择连接...", "", () => {
    void (async () => {
      const sid = getSessionId();
      if (!sid) return;
      const connections = await system.connections.listActiveBotAdapters();
      const selected = await showActiveBotAdapterPicker(
        connections,
        initialValue || widget._selectedConnectionId || "",
      );
      if (selected == null) return;
      widget._selectedConnectionId = selected;
      widget.label = activeBotAdapterButtonLabel(connections, selected);
      widget._zihuanTouched = true;
      widget._zihuanInlineKey = key;
      const pending = graphs.updateNode(sid, nodeDef.id, {
        inline_values: {
          [key]: selected && selected !== CONNECTION_PLACEHOLDER_VALUE ? selected : null,
        },
      });
      onMutated?.(pending);
      await pending;
    })();
  });
  widget.label = "加载中...";
  widget._selectedConnectionId = initialValue || CONNECTION_PLACEHOLDER_VALUE;
  widget._zihuanInlineKey = key;
  attachWidgetClickProxy(lNode, widget, () => {
    void widget.callback?.();
  });
  loadActiveBotAdapterOptions(widget, lNode, initialValue);
  return widget;
}

function setupConnectionSelectWidget(
  lNode: any,
  nodeDef: NodeDefinition,
  key: string,
  connectionKind: string,
  initialValue: string,
  getSessionId: () => string | null,
  onMutated?: WidgetMutationCallback,
): any {
  const widget = lNode.addWidget("button", "请选择连接...", "", () => {
    void (async () => {
      const sid = getSessionId();
      if (!sid) return;
      const connections = await getConnections();
      const selected = await showConnectionPicker(
        connections,
        connectionKind,
        widget._selectedConnectionId || CONNECTION_PLACEHOLDER_VALUE,
      );
      if (selected == null) return;
      widget._selectedConnectionId = selected;
      widget.label = connectionButtonLabel(connections, connectionKind, selected);
      widget._zihuanTouched = true;
      widget._zihuanInlineKey = key;
      const pending = graphs.updateNode(sid, nodeDef.id, {
        inline_values: {
          [key]: selected && selected !== CONNECTION_PLACEHOLDER_VALUE ? selected : null,
        },
      });
      onMutated?.(pending);
      await pending;
    })().catch((error) => {
      console.error("connection picker failed", error);
    });
  });
  widget.value = initialValue || CONNECTION_PLACEHOLDER_VALUE;
  widget._selectedConnectionId = initialValue || CONNECTION_PLACEHOLDER_VALUE;
  widget._zihuanInlineKey = key;
  attachWidgetClickProxy(lNode, widget, () => {
    void widget.callback?.();
  });
  loadConnectionOptions(widget, lNode, connectionKind, initialValue);
  return widget;
}
