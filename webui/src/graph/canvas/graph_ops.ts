import { LiteGraph } from "litegraph.js";
import { graphs } from "../../api/client";
import { logger } from "../../api/logger";
import type { EdgeDefinition, NodeDefinition, NodeGraphDefinition } from "../../api/types";
import { getBoundaryNodeColors, getDisabledNodeColors, getPortColor } from "../../ui/theme";
import { setupNodeWidgets } from "../widgets";
import type { BrainToolDefinition, EmbeddedFunctionConfig } from "../../ui/dialogs/types";
import type { CanvasFacade } from "./types";
import { resolveConcretePortType, visibleInputPorts } from "./type_utils";
import {
  DESC_BAND_HEIGHT,
  NODE_TITLE_HEIGHT,
  drawBindingBadges,
  drawHelpButton,
  findRegisteredType,
  truncateText,
} from "./rendering";
import { portTypeString } from "../registry";

export class CanvasGraphOps {
  constructor(private readonly canvas: CanvasFacade) {}

  private defaultInlineWidgetValue(typeName: string): unknown {
    switch (typeName) {
      case "Boolean":
        return false;
      case "Integer":
      case "Float":
        return 0;
      case "String":
      case "Password":
        return "";
      default:
        return undefined;
    }
  }

  private async commitActiveInlineEditor(): Promise<void> {
    const active = document.activeElement;
    if (!active) return;

    const isEditableElement = active instanceof HTMLInputElement
      || active instanceof HTMLTextAreaElement
      || (active as HTMLElement).isContentEditable === true;
    if (!isEditableElement) return;

    // LiteGraph commits widget edits on blur/change. When the user types an inline
    // value and immediately runs/validates, that callback may not have fired yet.
    // Force a blur and wait one microtask so the widget state is flushed first.
    (active as HTMLElement).blur();
    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, 0);
    });
  }

  async loadSession(sessionId: string): Promise<void> {
    const def = await graphs.get(sessionId);
    this.canvas.state = { sessionId, graph: def, dirty: false };
    this.canvas.history.reset(def);
    this.canvas.onHistoryChange?.();
    this.rebuildCanvas(def);
  }

  rebuildCanvas(def: NodeGraphDefinition): void {
    this.canvas._rebuilding = true;
    try {
      this.canvas.lGraph.clear();
      this.canvas.nodeMap.clear();
      for (const nodeDef of def.nodes) this.addLGraphNode(nodeDef);
      for (const edge of def.edges) this.connectLGraphEdge(edge);
      this.colorizeAllLinks();
      this.canvas.lGraph.setDirtyCanvas(true, true);
    } finally {
      this.canvas._rebuilding = false;
    }
  }

  async syncPositions(): Promise<void> {
    const sessionId = this.canvas.state.sessionId;
    if (!sessionId) return;
    const promises: Promise<unknown>[] = [];
    for (const [nodeId, node] of this.canvas.nodeMap) {
      if (!node.pos) continue;
      promises.push(
        graphs.updateNode(sessionId, nodeId, {
          x: node.pos[0] as number,
          y: node.pos[1] as number,
          width: node.size?.[0] as number | undefined,
          height: node.size?.[1] as number | undefined,
        }).catch(() => {}),
      );
    }
    await Promise.all(promises);
  }

  async reloadCurrentSession(): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid) return;
    const def = await graphs.get(sid);
    this.canvas.state.graph = def;
    this.rebuildCanvas(def);
  }

  async syncInlineWidgetValues(): Promise<void> {
    const sid = this.canvas.state.sessionId;
    const graph = this.canvas.state.graph;
    if (!sid || !graph) return;

    await this.commitActiveInlineEditor();

    let changed = false;
    const updatedGraph: NodeGraphDefinition = {
      ...graph,
      nodes: graph.nodes.map((nodeDef) => {
        const lNode = this.canvas.nodeMap.get(nodeDef.id);
        if (!lNode?.inputs?.length || !lNode?.widgets?.length) return nodeDef;

        let nextInlineValues: Record<string, unknown> | null = null;
        for (const input of lNode.inputs as Array<{ name?: string; widget?: { name?: string } | string; link?: unknown }>) {
          const portName = input?.name;
          if (!portName || input.link != null || !input.widget || nodeDef.port_bindings?.[portName]) continue;
          const widgetName = typeof input.widget === "object" ? input.widget.name : input.widget;
          if (!widgetName) continue;
          const widget = (lNode.widgets as Array<{ name?: string; value?: unknown; _zihuanTouched?: boolean }>).find(
            (candidate) => candidate?.name === widgetName
          );
          if (!widget) continue;
          const hadInlineValue = Object.prototype.hasOwnProperty.call(nodeDef.inline_values ?? {}, portName);
          const portDef = nodeDef.input_ports.find((port) => port.name === portName);
          const portType = portDef ? portTypeString(portDef.data_type) : "";

          const widgetValue = widget.value ?? "";
          const defaultValue = this.defaultInlineWidgetValue(portType);
          const differsFromDefault = defaultValue !== undefined && widgetValue !== defaultValue;
          if (!hadInlineValue && !widget._zihuanTouched && !differsFromDefault) continue;
          const existingValue = nodeDef.inline_values?.[portName];
          if (existingValue === widgetValue) continue;

          nextInlineValues ??= { ...(nodeDef.inline_values ?? {}) };
          nextInlineValues[portName] = widgetValue;
        }

        for (const widget of lNode.widgets as Array<{ value?: unknown; _zihuanInlineKey?: string; _zihuanTouched?: boolean }>) {
          const inlineKey = widget?._zihuanInlineKey;
          if (!inlineKey) continue;
          const existingValue = nodeDef.inline_values?.[inlineKey] ?? "";
          const widgetValue = widget.value ?? "";
          if (!widget._zihuanTouched && existingValue === widgetValue) continue;
          if (existingValue === widgetValue) continue;
          nextInlineValues ??= { ...(nodeDef.inline_values ?? {}) };
          nextInlineValues[inlineKey] = widgetValue;
        }

        if (!nextInlineValues) return nodeDef;
        changed = true;
        return { ...nodeDef, inline_values: nextInlineValues };
      }),
    };

    if (!changed) return;
    await graphs.put(sid, updatedGraph);
    this.canvas.state.graph = updatedGraph;
    this.canvas.history.push(updatedGraph);
    this.canvas.onHistoryChange?.();
  }

  trackWidgetMutation(pending?: Promise<unknown>): void {
    if (pending) {
      this.canvas._pendingWidgetMutations.add(pending);
      pending.finally(() => {
        this.canvas._pendingWidgetMutations.delete(pending);
      });
    }
    this.canvas.onGraphDirty?.();
  }

  async flushPendingWidgetMutations(): Promise<void> {
    while (this.canvas._pendingWidgetMutations.size > 0) {
      const pendingNow = Array.from(this.canvas._pendingWidgetMutations);
      await Promise.allSettled(pendingNow);
    }
  }

  onWidgetMutated(pending?: Promise<unknown>): void {
    this.trackWidgetMutation(pending);
    if (this.canvas._widgetMutationTimer !== null) {
      clearTimeout(this.canvas._widgetMutationTimer);
    }
    this.canvas._widgetMutationTimer = setTimeout(async () => {
      this.canvas._widgetMutationTimer = null;
      const sid = this.canvas.state.sessionId;
      if (!sid) return;
      try {
        const updated = await graphs.get(sid);
        this.canvas.state.graph = updated;
        this.canvas.history.push(updated);
        this.canvas.onHistoryChange?.();
      } catch {}
    }, 500);
  }

  onNodesMoved(): void {
    if (this.canvas._nodeMoveTimer !== null) {
      clearTimeout(this.canvas._nodeMoveTimer);
    }
    this.canvas._nodeMoveTimer = setTimeout(async () => {
      this.canvas._nodeMoveTimer = null;
      const sid = this.canvas.state.sessionId;
      if (!sid || !this.canvas.state.graph) return;
      const updatedNodes = this.canvas.state.graph.nodes.map((nodeDef) => {
        const lNode = this.canvas.nodeMap.get(nodeDef.id) as any;
        if (!lNode?.pos) return nodeDef;
        return {
          ...nodeDef,
          position: { x: Math.round(lNode.pos[0]), y: Math.round(lNode.pos[1]) },
        };
      });
      const updatedGraph = { ...this.canvas.state.graph, nodes: updatedNodes };
      try {
        await graphs.put(sid, updatedGraph);
        this.canvas.state.graph = updatedGraph;
        this.canvas.history.push(updatedGraph);
        this.canvas.onHistoryChange?.();
      } catch (e) {
        console.error("[Canvas] onNodesMoved put failed:", e);
      }
    }, 300);
  }

  canUndo(): boolean {
    return this.canvas.history.canUndo();
  }

  canRedo(): boolean {
    return this.canvas.history.canRedo();
  }

  async undo(): Promise<void> {
    const snapshot = this.canvas.history.undo();
    if (!snapshot) return;
    await this.applyHistorySnapshot(snapshot);
  }

  async redo(): Promise<void> {
    const snapshot = this.canvas.history.redo();
    if (!snapshot) return;
    await this.applyHistorySnapshot(snapshot);
  }

  private async applyHistorySnapshot(snapshot: NodeGraphDefinition): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid) return;
    try {
      await graphs.put(sid, snapshot);
      this.canvas.state.graph = snapshot;
      this.canvas.state.dirty = true;
      this.canvas.onGraphDirty?.();
      this.rebuildCanvas(snapshot);
      this.canvas.onHistoryChange?.();
    } catch (e) {
      console.error("[Canvas] applyHistorySnapshot failed:", e);
    }
  }

  bindGraphEvents(): void {
    (this.canvas.lGraph as any).onAfterExecute = () => {};
    this.canvas.lGraph.onNodeAdded = (node: any) => this.onNodeAdded(node);
    (this.canvas.lGraph as any).onNodeRemoved = (node: any) => this.onNodeRemoved(node);
    (this.canvas.lGraph as any).onConnectionChange = (node: any) => this.onConnectionChanged(node);
  }

  private addLGraphNode(nodeDef: NodeDefinition): void {
    const typeKey = findRegisteredType(nodeDef.node_type);
    if (!typeKey) {
      console.warn(`[Canvas] Unknown node type: ${nodeDef.node_type}`);
      return;
    }
    const node = LiteGraph.createNode(typeKey) as any;
    if (!node) return;

    node.inputs = [];
    node.outputs = [];
    const visibleInPorts = visibleInputPorts(nodeDef.input_ports);
    for (const port of visibleInPorts) node.addInput(port.name, portTypeString(port.data_type));
    for (const port of nodeDef.output_ports) node.addOutput(port.name, portTypeString(port.data_type));

    if (node.inputs) {
      for (let i = 0; i < visibleInPorts.length; i++) {
        const port = visibleInPorts[i];
        const typeStr = portTypeString(port.data_type);
        const resolvedType = typeStr === "Any" && this.canvas.state.graph
          ? resolveConcretePortType(this.canvas.state.graph, nodeDef.id, port.name, true)
          : typeStr;
        const col = getPortColor(resolvedType);
        const hasInlineValue = nodeDef.inline_values != null && nodeDef.inline_values[port.name] != null;
        const hasPortBinding = nodeDef.port_bindings != null && nodeDef.port_bindings[port.name] != null;
        node.inputs[i].color_on = col;
        node.inputs[i].color_off = (!hasInlineValue && !hasPortBinding && port.required) ? "#e74c3c" : col;
      }
    }

    if (node.outputs) {
      for (let i = 0; i < nodeDef.output_ports.length; i++) {
        const port = nodeDef.output_ports[i];
        const typeStr = portTypeString(port.data_type);
        const resolvedType = typeStr === "Any" && this.canvas.state.graph
          ? resolveConcretePortType(this.canvas.state.graph, nodeDef.id, port.name, false)
          : typeStr;
        const col = getPortColor(resolvedType);
        node.outputs[i].color_on = col;
        node.outputs[i].color_off = col;
      }
    }

    const portBindings = nodeDef.port_bindings ?? {};
    node._portBindings = portBindings;
    if (node.inputs) {
      for (let i = 0; i < visibleInPorts.length; i++) {
        const binding = portBindings[visibleInPorts[i].name];
        if (!binding) continue;
        const dotColor = binding.kind === "hyperparameter" ? "#e67e22" : "#1abc9c";
        node.inputs[i].color_on = dotColor;
        node.inputs[i].color_off = dotColor;
      }
    }

    node._helpVisible = false;
    node.onMouseEnter = () => {
      node._helpVisible = true;
      this.canvas.lGraph.setDirtyCanvas(true, false);
    };
    node.onMouseLeave = () => {
      node._helpVisible = false;
      this.canvas.lGraph.setDirtyCanvas(true, false);
    };
    node.onDrawForeground = function (this: any, ctx: CanvasRenderingContext2D) {
      drawBindingBadges.call(this, ctx);
      drawHelpButton.call(this, ctx);
    };
    node.onMouseDown = (e: MouseEvent, pos: [number, number]): boolean | undefined => {
      const btnX = node.size[0] - 12;
      const btnY = -NODE_TITLE_HEIGHT / 2;
      const dx = pos[0] - btnX;
      const dy = pos[1] - btnY;
      if (Math.sqrt(dx * dx + dy * dy) <= 10) {
        e.preventDefault();
        e.stopPropagation();
        this.canvas.showNodeHelpDialog(node);
        return true;
      }
      return undefined;
    };

    node.id = nodeDef.id;
    node.title = nodeDef.name;
    node.onDrawTitleText = function (
      this: any,
      ctx: CanvasRenderingContext2D,
      titleHeight: number,
      size: [number, number],
      _scale: number,
      font: string,
      selected: boolean,
    ) {
      ctx.font = font;
      ctx.textAlign = "left";
      ctx.fillStyle = selected
        ? (LiteGraph as any).NODE_SELECTED_TITLE_COLOR
        : (this.constructor.title_text_color || (LiteGraph as any).NODE_TITLE_COLOR);
      const title = String(this.title);
      const maxWidth = size[0] - titleHeight - 8;
      ctx.fillText(truncateText(ctx, title, maxWidth), titleHeight, LiteGraph.NODE_TITLE_TEXT_Y - titleHeight);
    };

    if (nodeDef.position) node.pos = [nodeDef.position.x, nodeDef.position.y];
    node.zihuanId = nodeDef.id;

    if (nodeDef.node_type === "function_inputs" || nodeDef.node_type === "function_outputs") {
      const boundaryColors = getBoundaryNodeColors();
      node.color = boundaryColors.header;
      node.bgcolor = boundaryColors.bg;
      node.block_delete = true;
    }

    if (nodeDef.disabled) {
      const disabledColors = getDisabledNodeColors();
      node.color = disabledColors.header;
      node.bgcolor = disabledColors.bg;
    }
    node._zihuanDisabled = !!nodeDef.disabled;

    if (nodeDef.node_type === "function") {
      const funcCfg = nodeDef.inline_values?.function_config as EmbeddedFunctionConfig | undefined;
      const description = funcCfg?.description?.trim() ?? "";
      if (description) {
        const BaseClass = Object.getPrototypeOf(node).constructor;
        const DescClass = class extends BaseClass {};
        DescClass.title = BaseClass.title;
        DescClass.desc = BaseClass.desc;
        (DescClass as any).zihuanTypeId = (BaseClass as any).zihuanTypeId;
        (DescClass as any).title_text_color = (BaseClass as any).title_text_color;
        (DescClass as any).title_color = (BaseClass as any).title_color;
        (DescClass as any).min_height = (BaseClass as any).min_height;
        (DescClass as any).slot_start_y = DESC_BAND_HEIGHT;
        Object.setPrototypeOf(node, DescClass.prototype);
        node._descHeight = DESC_BAND_HEIGHT;
        node._functionDescription = description;
      }
    }

    this.canvas.lGraph.add(node);
    this.canvas.nodeMap.set(nodeDef.id, node);

    setupNodeWidgets(
      node,
      nodeDef,
      () => this.canvas.state.sessionId,
      () => { this.reloadCurrentSession().catch(console.error); },
      (
        parentNodeDef: NodeDefinition,
        mode: "function" | "brain-tool",
        toolIndex?: number,
        toolDef?: BrainToolDefinition,
        functionConfig?: EmbeddedFunctionConfig,
      ) => {
        this.canvas.enterSubgraph(parentNodeDef, mode, toolIndex, toolDef, functionConfig).catch(console.error);
      },
      (pending?: Promise<unknown>) => { this.onWidgetMutated(pending); },
    );

    if (nodeDef.size) {
      node.size = [nodeDef.size.width, nodeDef.size.height];
    }
  }

  private connectLGraphEdge(edge: EdgeDefinition): void {
    const fromNode = this.canvas.nodeMap.get(edge.from_node_id) as any;
    const toNode = this.canvas.nodeMap.get(edge.to_node_id) as any;
    if (!fromNode || !toNode) {
      logger.warn(`[Canvas] connectLGraphEdge: node not found — from=${edge.from_node_id} to=${edge.to_node_id}`);
      return;
    }
    const fromDef = this.canvas.state.graph?.nodes.find((node) => node.id === edge.from_node_id);
    const toDef = this.canvas.state.graph?.nodes.find((node) => node.id === edge.to_node_id);
    if (!fromDef || !toDef) return;
    const fromPortIdx = fromDef.output_ports.findIndex((port) => port.name === edge.from_port);
    const toPortIdx = visibleInputPorts(toDef.input_ports).findIndex((port) => port.name === edge.to_port);
    if (fromPortIdx < 0 || toPortIdx < 0) {
      logger.warn(`[Canvas] connectLGraphEdge: port not found — ${edge.from_node_id}.${edge.from_port}(out=${fromPortIdx}) -> ${edge.to_node_id}.${edge.to_port}(in=${toPortIdx})`);
      return;
    }
    fromNode.connect(fromPortIdx, toNode, toPortIdx);
  }

  private colorizeAllLinks(): void {
    const links = this.canvas.lGraph.links;
    if (!links) return;
    for (const link of Object.values(links) as any[]) {
      if (!link) continue;
      const originNode = this.canvas.lGraph.getNodeById(link.origin_id) as any;
      if (!originNode) continue;
      const originDef = this.canvas.state.graph?.nodes.find((node) => node.id === originNode.zihuanId);
      if (!originDef) continue;
      const port = originDef.output_ports[link.origin_slot];
      if (!port) continue;
      const resolvedType = this.canvas.state.graph
        ? resolveConcretePortType(this.canvas.state.graph, originDef.id, port.name, false)
        : portTypeString(port.data_type);
      link.color = getPortColor(resolvedType);
    }
  }

  private onNodeAdded(node: any): void {
    if (node.zihuanId) return;
    const sessionId = this.canvas.state.sessionId;
    if (!sessionId) return;
    const typeId: string = (node.constructor as any).zihuanTypeId ?? node.type ?? "";
    const x: number = node.pos?.[0] ?? 0;
    const y: number = node.pos?.[1] ?? 0;
    graphs.addNode(sessionId, typeId, node.title ?? undefined, x, y)
      .then(async (result) => {
        node.zihuanId = result.id;
        this.canvas.nodeMap.set(result.id, node);
        this.canvas.state.dirty = true;
        try {
          const updated = await graphs.get(sessionId);
          this.canvas.state.graph = updated;
          this.canvas.history.push(updated);
          this.canvas.onHistoryChange?.();
        } catch {}
      })
      .catch((e) => console.error("[Canvas] addNode failed:", e));
  }

  private onNodeRemoved(node: any): void {
    if (this.canvas._rebuilding) return;
    const sessionId = this.canvas.state.sessionId;
    const nodeId: string | undefined = node.zihuanId;
    if (!sessionId || !nodeId) return;
    if (nodeId === "__function_inputs__" || nodeId === "__function_outputs__") return;
    this.canvas.nodeMap.delete(nodeId);
    graphs.deleteNode(sessionId, nodeId)
      .then(async () => {
        this.canvas.state.dirty = true;
        try {
          const updated = await graphs.get(sessionId);
          this.canvas.state.graph = updated;
          this.canvas.history.push(updated);
          this.canvas.onHistoryChange?.();
        } catch {}
      })
      .catch((e) => console.error("[Canvas] deleteNode failed:", e));
  }

  private onConnectionChanged(_node: any): void {
    if (this.canvas._rebuilding) return;
    const sessionId = this.canvas.state.sessionId;
    if (!sessionId) return;
    const edgeList: any[] = this.canvas.lGraph.links ? Object.values(this.canvas.lGraph.links) : [];
    const edgeDefs: EdgeDefinition[] = [];
    for (const link of edgeList) {
      if (!link) continue;
      const originNode = this.canvas.lGraph.getNodeById(link.origin_id) as any;
      const targetNode = this.canvas.lGraph.getNodeById(link.target_id) as any;
      if (!originNode?.zihuanId || !targetNode?.zihuanId) continue;
      const fromDef = this.canvas.state.graph?.nodes.find((node) => node.id === originNode.zihuanId);
      const toDef = this.canvas.state.graph?.nodes.find((node) => node.id === targetNode.zihuanId);
      if (!fromDef || !toDef) continue;
      const fromPort = fromDef.output_ports[link.origin_slot];
      const toPort = visibleInputPorts(toDef.input_ports)[link.target_slot];
      if (!fromPort || !toPort) continue;
      edgeDefs.push({
        from_node_id: originNode.zihuanId as string,
        from_port: fromPort.name,
        to_node_id: targetNode.zihuanId as string,
        to_port: toPort.name,
      });
    }

    if (!this.canvas.state.graph) return;
    const updatedGraph = { ...this.canvas.state.graph, edges: edgeDefs };
    this.canvas.state.graph = updatedGraph;
    graphs.put(sessionId, updatedGraph)
      .then(() => {
        this.canvas.history.push(updatedGraph);
        this.canvas.onHistoryChange?.();
      })
      .catch((e) => console.error("[Canvas] put graph (edges) failed:", e));
    this.canvas.state.dirty = true;
    this.canvas.onGraphDirty?.();
    this.colorizeAllLinks();
    this.canvas.lGraph.setDirtyCanvas(true, false);
  }
}
