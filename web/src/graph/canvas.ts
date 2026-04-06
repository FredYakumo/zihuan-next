// Litegraph canvas wrapper — bridges LiteGraph and the Zihuan API

import { LGraph, LGraphCanvas, LiteGraph } from "@comfyorg/litegraph";
import { graphs } from "../api/client";
import type { NodeGraphDefinition, NodeDefinition, EdgeDefinition } from "../api/types";
import { setupNodeWidgets } from "./widgets";
import { portTypeString } from "./registry";
import type { BrainToolDefinition, EmbeddedFunctionConfig } from "../ui/dialogs";

export interface CanvasState {
  sessionId: string | null;
  graph: NodeGraphDefinition | null;
  dirty: boolean;
}

interface SubgraphStackEntry {
  label: string;
  /** The session to return to when exiting */
  parentSessionId: string;
  /** Virtual session ID that holds the subgraph content */
  virtualSessionId: string;
  /** Called to persist the modified subgraph back to the parent node */
  saveBack: (modifiedGraph: NodeGraphDefinition) => Promise<void>;
}

export class ZihuanCanvas {
  private lGraph: InstanceType<typeof LGraph>;
  private lCanvas: InstanceType<typeof LGraphCanvas>;
  private state: CanvasState = { sessionId: null, graph: null, dirty: false };
  /** Map from backend node id → LGraph node */
  private nodeMap = new Map<string, any>();
  /** Subgraph navigation stack */
  private subgraphStack: SubgraphStackEntry[] = [];
  /** In-memory node clipboard for copy/paste */
  private nodeClipboard: NodeDefinition[] = [];

  /** Called whenever the breadcrumb navigation path changes */
  onNavigationChange?: (labels: string[]) => void;

  /**
   * Called when the user right-clicks on an empty area of the canvas.
   * Receives the position in graph coordinates where the node should be placed.
   */
  onAddNodeRequest?: (graphX: number, graphY: number) => void;

  constructor(canvasEl: HTMLCanvasElement) {
    this.lGraph = new (LGraph as any)();
    this.lCanvas = new (LGraphCanvas as any)(canvasEl, this.lGraph);

    // Wire up LiteGraph change callbacks
    this.lGraph.onAfterExecute = () => {};

    // Listen to node add/remove/connect events
    this.lGraph.onNodeAdded = (node: any) => this.onNodeAdded(node);
    this.lGraph.onNodeRemoved = (node: any) => this.onNodeRemoved(node);
    this.lGraph.onConnectionChange = (node: any) => this.onConnectionChanged(node);

    // Right-click context menu (capture phase so we preempt LiteGraph's own handler).
    canvasEl.addEventListener("contextmenu", (e: MouseEvent) => {
      const [gx, gy] = (this.lCanvas as any).graph_mouse as [number, number];
      const node = this.lGraph.getNodeOnPos(gx, gy);

      // If right-clicking on a node's input slot, show port-binding menu instead.
      if (node) {
        const nx = (node as any).pos[0] as number;
        const ny = (node as any).pos[1] as number;
        const found = (node as any).getSlotInPosition(gx - nx, gy - ny) as
          | { slot: number; input?: unknown; output?: unknown }
          | null;
        if (found && found.input) {
          const portName = ((node as any).inputs?.[found.slot]?.name ?? "") as string;
          if (portName) {
            e.preventDefault();
            e.stopPropagation();
            this.showPortBindingMenu(node, found.slot, portName, e);
            return;
          }
        }
      }

      // In all other cases show our custom canvas context menu.
      e.preventDefault();
      e.stopPropagation();
      this.showCanvasContextMenu(e, gx, gy);
    }, { capture: true });
  }

  get sessionId(): string | null {
    return this.state.sessionId;
  }

  /**
   * Return the current viewport center in graph coordinates.
   * Used to place a new node near the visible area when triggered from a button.
   */
  graphCenterPos(): { x: number; y: number } {
    const ds = (this.lCanvas as any).ds as { offset: [number, number]; scale: number } | undefined;
    const canvasEl = (this.lCanvas as any).canvas as HTMLCanvasElement | undefined;
    if (!ds || !canvasEl) return { x: 100, y: 100 };
    const w = canvasEl.width / (window.devicePixelRatio || 1);
    const h = canvasEl.height / (window.devicePixelRatio || 1);
    return {
      x: -ds.offset[0] + w / 2 / ds.scale,
      y: -ds.offset[1] + h / 2 / ds.scale,
    };
  }

  /** Clear the canvas and discard session state (e.g. when the last tab is closed). */
  clearCanvas(): void {
    this.state = { sessionId: null, graph: null, dirty: false };
    this.nodeMap.clear();
    this.subgraphStack = [];
    this.lGraph.clear();
    this.onNavigationChange?.([]);
  }

  /** Load a graph session from the API into the LiteGraph canvas. */
  async loadSession(sessionId: string): Promise<void> {
    const def = await graphs.get(sessionId);
    this.state = { sessionId, graph: def, dirty: false };
    this.rebuildCanvas(def);
  }

  /** Rebuild the LiteGraph canvas from a NodeGraphDefinition. */
  public rebuildCanvas(def: NodeGraphDefinition): void {
    this.lGraph.clear();
    this.nodeMap.clear();

    // Add nodes
    for (const nodeDef of def.nodes) {
      this.addLGraphNode(nodeDef);
    }

    // Add edges
    for (const edge of def.edges) {
      this.connectLGraphEdge(edge);
    }

    this.lGraph.setDirtyCanvas(true, true);
  }

  private addLGraphNode(nodeDef: NodeDefinition): void {
    const typeKey = findRegisteredType(nodeDef.node_type);
    if (!typeKey) {
      console.warn(`[Canvas] Unknown node type: ${nodeDef.node_type}`);
      return;
    }
    const node = LiteGraph.createNode(typeKey) as any;
    if (!node) return;

    // Sync ports from the backend definition (may differ from static registry ports).
    // Clear arrays directly — removeInput/removeOutput require a graph reference
    // which isn't available yet (node hasn't been added to the lGraph).
    node.inputs = [];
    node.outputs = [];
    for (const p of nodeDef.input_ports) {
      node.addInput(p.name, portTypeString(p.data_type as string | object));
    }
    for (const p of nodeDef.output_ports) {
      node.addOutput(p.name, portTypeString(p.data_type as string | object));
    }

    // Visual indicator for bound ports
    if (node.inputs) {
      for (let i = 0; i < nodeDef.input_ports.length; i++) {
        const portName = nodeDef.input_ports[i].name;
        const binding = nodeDef.port_bindings?.[portName];
        if (binding) {
          const prefix = binding.kind === "Hyperparameter" ? "↑" : "⟲";
          node.inputs[i].label = `${portName} [${prefix}${binding.name}]`;
        }
      }
    }

    node.id = nodeDef.id;
    node.title = nodeDef.name;

    if (nodeDef.position) {
      node.pos = [nodeDef.position.x, nodeDef.position.y];
    }
    if (nodeDef.size) {
      node.size = [nodeDef.size.width, nodeDef.size.height];
    }

    // Store backend id on the litegraph node
    node.zihuanId = nodeDef.id;

    this.lGraph.add(node);
    this.nodeMap.set(nodeDef.id, node);

    // Set up inline value widgets and special editor buttons
    setupNodeWidgets(
      node,
      nodeDef,
      () => this.state.sessionId,
      () => { this.reloadCurrentSession().catch(console.error); },
      (parentNodeDef, mode, toolIndex, toolDef, functionConfig) => {
        this.enterSubgraph(parentNodeDef, mode, toolIndex, toolDef, functionConfig).catch(console.error);
      }
    );
  }

  private connectLGraphEdge(edge: EdgeDefinition): void {
    const fromNode = this.nodeMap.get(edge.from_node_id) as any;
    const toNode = this.nodeMap.get(edge.to_node_id) as any;
    if (!fromNode || !toNode) return;

    const fromDef = this.state.graph?.nodes.find((n) => n.id === edge.from_node_id);
    const toDef = this.state.graph?.nodes.find((n) => n.id === edge.to_node_id);
    if (!fromDef || !toDef) return;

    const fromPortIdx = fromDef.output_ports.findIndex((p) => p.name === edge.from_port);
    const toPortIdx = toDef.input_ports.findIndex((p) => p.name === edge.to_port);
    if (fromPortIdx < 0 || toPortIdx < 0) return;

    fromNode.connect(fromPortIdx, toNode, toPortIdx);
  }

  // ─── LiteGraph event handlers ─────────────────────────────────────────────

  private onNodeAdded(node: any): void {
    // Only handle nodes that came from the UI (not from rebuildCanvas)
    if (node.zihuanId) return;

    const sessionId = this.state.sessionId;
    if (!sessionId) return;

    const typeId: string = (node.constructor as any).zihuanTypeId ?? node.type ?? "";
    const x: number = node.pos?.[0] ?? 0;
    const y: number = node.pos?.[1] ?? 0;

    graphs
      .addNode(sessionId, typeId, node.title ?? undefined, x, y)
      .then((result) => {
        node.zihuanId = result.id;
        this.nodeMap.set(result.id, node);
        this.state.dirty = true;
      })
      .catch((e) => console.error("[Canvas] addNode failed:", e));
  }

  private onNodeRemoved(node: any): void {
    const sessionId = this.state.sessionId;
    const nodeId: string | undefined = node.zihuanId;
    if (!sessionId || !nodeId) return;

    this.nodeMap.delete(nodeId);
    graphs
      .deleteNode(sessionId, nodeId)
      .then(() => {
        this.state.dirty = true;
      })
      .catch((e) => console.error("[Canvas] deleteNode failed:", e));
  }

  private onConnectionChanged(node: any): void {
    // Re-sync the full edge list from the canvas to the backend
    const sessionId = this.state.sessionId;
    if (!sessionId) return;

    // Collect all current edges from the litegraph
    const edgeList: any[] = this.lGraph.links ? Object.values(this.lGraph.links) : [];

    const edgeDefs: EdgeDefinition[] = [];
    for (const link of edgeList) {
      if (!link) continue;
      const originNode = this.lGraph.getNodeById(link.origin_id) as any;
      const targetNode = this.lGraph.getNodeById(link.target_id) as any;
      if (!originNode?.zihuanId || !targetNode?.zihuanId) continue;

      const fromDef = this.state.graph?.nodes.find((n) => n.id === originNode.zihuanId);
      const toDef = this.state.graph?.nodes.find((n) => n.id === targetNode.zihuanId);
      if (!fromDef || !toDef) continue;

      const fromPort = fromDef.output_ports[link.origin_slot];
      const toPort = toDef.input_ports[link.target_slot];
      if (!fromPort || !toPort) continue;

      edgeDefs.push({
        from_node_id: originNode.zihuanId as string,
        from_port: fromPort.name,
        to_node_id: targetNode.zihuanId as string,
        to_port: toPort.name,
      });
    }

    // Update the graph definition with new edges and PUT to backend
    if (this.state.graph) {
      const updatedGraph = { ...this.state.graph, edges: edgeDefs };
      this.state.graph = updatedGraph;
      graphs
        .put(sessionId, updatedGraph)
        .catch((e) => console.error("[Canvas] put graph (edges) failed:", e));
      this.state.dirty = true;
    }
  }

  /** Sync node positions after drag to the backend. */
  syncPositions(): void {
    const sessionId = this.state.sessionId;
    if (!sessionId) return;

    for (const [nodeId, node] of this.nodeMap) {
      if (node.pos) {
        graphs
          .updateNode(sessionId, nodeId, {
            x: node.pos[0] as number,
            y: node.pos[1] as number,
            width: node.size?.[0] as number | undefined,
            height: node.size?.[1] as number | undefined,
          })
          .catch(() => {});
      }
    }
  }

  /** Resize the LiteGraph canvas, updating both the DOM element and LiteGraph's internal viewport.
   * LiteGraph expects physical pixel dimensions (logical × devicePixelRatio) for correct HiDPI rendering. */
  resize(width: number, height: number): void {
    const dpr = window.devicePixelRatio || 1;
    (this.lCanvas as any).resize(Math.round(width * dpr), Math.round(height * dpr));
  }

  startPositionSync(intervalMs = 2000): () => void {
    const id = setInterval(() => this.syncPositions(), intervalMs);
    return () => clearInterval(id);
  }

  get isInSubgraph(): boolean {
    return this.subgraphStack.length > 0;
  }

  /** Reload the current session from the backend and redraw. */
  async reloadCurrentSession(): Promise<void> {
    const sid = this.state.sessionId;
    if (!sid) return;
    const def = await graphs.get(sid);
    this.state.graph = def;
    this.rebuildCanvas(def);
  }

  /** Enter a function or brain-tool subgraph by creating a virtual session. */
  async enterSubgraph(
    parentNodeDef: NodeDefinition,
    mode: "function" | "brain-tool",
    toolIndex?: number,
    toolDef?: BrainToolDefinition,
    functionConfig?: EmbeddedFunctionConfig
  ): Promise<void> {
    const parentSessionId = this.state.sessionId;
    if (!parentSessionId) return;

    // Determine the subgraph to display
    let subgraphDef: NodeGraphDefinition;
    let label: string;

    if (mode === "function" && functionConfig) {
      subgraphDef = functionConfig.subgraph;
      label = functionConfig.name || parentNodeDef.name;
    } else if (mode === "brain-tool" && toolDef != null) {
      subgraphDef = toolDef.subgraph;
      label = `${parentNodeDef.name} / ${toolDef.name}`;
    } else {
      return;
    }

    // Create a virtual graph session to hold the subgraph
    const tab = await graphs.create();
    const virtualSessionId = tab.id;
    await graphs.put(virtualSessionId, subgraphDef);

    // Define how to save the subgraph back to the parent node
    const saveBack = async (modifiedGraph: NodeGraphDefinition): Promise<void> => {
      if (mode === "function" && functionConfig) {
        const updatedConfig: EmbeddedFunctionConfig = { ...functionConfig, subgraph: modifiedGraph };
        const parentGraph = await graphs.get(parentSessionId);
        const nodeIdx = parentGraph.nodes.findIndex(n => n.id === parentNodeDef.id);
        if (nodeIdx >= 0) {
          parentGraph.nodes[nodeIdx] = {
            ...parentGraph.nodes[nodeIdx],
            inline_values: {
              ...parentGraph.nodes[nodeIdx].inline_values,
              function_config: updatedConfig as unknown as Record<string, unknown>,
            },
          };
          await graphs.put(parentSessionId, parentGraph);
        }
      } else if (mode === "brain-tool" && toolDef != null && toolIndex != null) {
        const parentGraph = await graphs.get(parentSessionId);
        const nodeIdx = parentGraph.nodes.findIndex(n => n.id === parentNodeDef.id);
        if (nodeIdx >= 0) {
          const tools: BrainToolDefinition[] = JSON.parse(
            JSON.stringify(parentGraph.nodes[nodeIdx].inline_values?.["tools_config"] ?? [])
          );
          if (tools[toolIndex]) {
            tools[toolIndex] = { ...tools[toolIndex], subgraph: modifiedGraph };
          }
          parentGraph.nodes[nodeIdx] = {
            ...parentGraph.nodes[nodeIdx],
            inline_values: {
              ...parentGraph.nodes[nodeIdx].inline_values,
              tools_config: tools as unknown as unknown[],
            },
          };
          await graphs.put(parentSessionId, parentGraph);
        }
      }
    };

    this.subgraphStack.push({ label, parentSessionId, virtualSessionId, saveBack });
    this._notifyNavigation();

    // Load the virtual session
    await this.loadSession(virtualSessionId);
  }

  /** Exit the current subgraph, saving back to the parent. */
  async exitSubgraph(): Promise<void> {
    if (this.subgraphStack.length === 0) return;

    const entry = this.subgraphStack[this.subgraphStack.length - 1];

    // Save current subgraph state back
    if (this.state.sessionId === entry.virtualSessionId) {
      const currentGraph = await graphs.get(entry.virtualSessionId);
      await entry.saveBack(currentGraph);
    }

    // Delete the virtual session
    try {
      await graphs.delete(entry.virtualSessionId);
    } catch {
      // Non-fatal if delete fails
    }

    this.subgraphStack.pop();
    this._notifyNavigation();

    // Reload the parent session
    await this.loadSession(entry.parentSessionId);
  }

  private _notifyNavigation(): void {
    if (!this.onNavigationChange) return;
    const labels = this.subgraphStack.map(e => e.label);
    this.onNavigationChange(labels);
  }

  // ─── Canvas context menu ──────────────────────────────────────────────────

  private showCanvasContextMenu(event: MouseEvent, graphX: number, graphY: number): void {
    document.getElementById("zh-canvas-menu")?.remove();

    const selectedNodes: any[] = Object.values((this.lGraph as any).selected_nodes ?? {});
    const hasSelection = selectedNodes.length > 0;
    const hasClipboard = this.nodeClipboard.length > 0;

    const menu = document.createElement("div");
    menu.id = "zh-canvas-menu";
    menu.style.cssText = `
      position:fixed;z-index:10000;
      left:${event.clientX}px;top:${event.clientY}px;
      background:#1a1a2e;border:1px solid #2a2a4a;border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.6);
      font-family:sans-serif;font-size:13px;color:#e0e0e0;min-width:170px;overflow:hidden;
    `;

    const makeItem = (label: string, enabled: boolean, onClick: () => void) => {
      const item = document.createElement("div");
      item.textContent = label;
      item.style.cssText = `padding:8px 14px;cursor:${enabled ? "pointer" : "default"};border-bottom:1px solid #1a2a4a;color:${enabled ? "#e0e0e0" : "#555"};`;
      if (enabled) {
        item.addEventListener("mouseenter", () => { item.style.background = "#1a3a6e"; });
        item.addEventListener("mouseleave", () => { item.style.background = ""; });
        item.addEventListener("click", () => { menu.remove(); onClick(); });
      }
      menu.appendChild(item);
    };

    makeItem("新建节点", true, () => {
      this.onAddNodeRequest?.(graphX, graphY);
    });

    makeItem("复制", hasSelection, () => {
      this.copySelectedNodes();
    });

    makeItem("粘贴", hasClipboard, () => {
      this.pasteNodes(graphX, graphY).catch(console.error);
    });

    makeItem("删除", hasSelection, () => {
      this.deleteSelectedNodes().catch(console.error);
    });

    document.body.appendChild(menu);

    const dismiss = (e: MouseEvent) => {
      if (!menu.contains(e.target as Node)) {
        menu.remove();
        document.removeEventListener("click", dismiss);
      }
    };
    setTimeout(() => document.addEventListener("click", dismiss), 0);
  }

  /** Copy currently selected nodes into the in-memory clipboard. */
  private copySelectedNodes(): void {
    const selectedLNodes: any[] = Object.values((this.lGraph as any).selected_nodes ?? {});
    if (selectedLNodes.length === 0) return;
    const defs: NodeDefinition[] = [];
    for (const lNode of selectedLNodes) {
      const nodeId = lNode.zihuanId as string | undefined;
      if (!nodeId) continue;
      const def = this.state.graph?.nodes.find((n) => n.id === nodeId);
      if (def) defs.push(def);
    }
    this.nodeClipboard = defs;
  }

  /** Paste clipboard nodes, offset by 20 px from the given graph position. */
  private async pasteNodes(graphX: number, graphY: number): Promise<void> {
    const sid = this.state.sessionId;
    if (!sid || this.nodeClipboard.length === 0) return;

    // Determine bounding box of clipboard nodes to anchor the paste position.
    const xs = this.nodeClipboard.map((n) => n.position?.x ?? 0);
    const ys = this.nodeClipboard.map((n) => n.position?.y ?? 0);
    const minX = Math.min(...xs);
    const minY = Math.min(...ys);
    const OFFSET = 20;

    for (const def of this.nodeClipboard) {
      const dx = (def.position?.x ?? 0) - minX;
      const dy = (def.position?.y ?? 0) - minY;
      await graphs.addNode(sid, def.node_type, def.name, graphX + dx + OFFSET, graphY + dy + OFFSET);
    }
    await this.reloadCurrentSession();
    this.state.dirty = true;
  }

  /** Delete all currently selected nodes from the canvas and backend. */
  private async deleteSelectedNodes(): Promise<void> {
    const selectedLNodes: any[] = Object.values((this.lGraph as any).selected_nodes ?? {});
    if (selectedLNodes.length === 0) return;
    // Remove each selected node via LiteGraph — onNodeRemoved will sync to backend.
    for (const lNode of [...selectedLNodes]) {
      this.lGraph.remove(lNode);
    }
  }

  // ─── Port binding context menu ────────────────────────────────────────────

  private showPortBindingMenu(
    lNode: any,
    _slotIndex: number,
    portName: string,
    event: MouseEvent
  ): void {
    document.getElementById("zh-port-menu")?.remove();

    const menu = document.createElement("div");
    menu.id = "zh-port-menu";
    menu.style.cssText = `
      position:fixed;z-index:10000;
      left:${event.clientX}px;top:${event.clientY}px;
      background:#1a1a2e;border:1px solid #2a2a4a;border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.6);
      font-family:sans-serif;font-size:13px;color:#e0e0e0;min-width:170px;overflow:hidden;
    `;

    const makeItem = (label: string, onClick: () => void) => {
      const item = document.createElement("div");
      item.textContent = label;
      item.style.cssText = "padding:8px 14px;cursor:pointer;border-bottom:1px solid #1a2a4a;";
      item.addEventListener("mouseenter", () => { item.style.background = "#1a3a6e"; });
      item.addEventListener("mouseleave", () => { item.style.background = ""; });
      item.addEventListener("click", () => { menu.remove(); onClick(); });
      menu.appendChild(item);
    };

    makeItem("绑定超参数...", () => {
      this.showHPPicker(lNode, portName, event).catch(console.error);
    });
    makeItem("绑定变量...", () => {
      this.showVarPicker(lNode, portName, event).catch(console.error);
    });
    makeItem("清除绑定", () => {
      this.clearPortBinding(lNode, portName).catch(console.error);
    });

    document.body.appendChild(menu);

    const dismiss = (e: MouseEvent) => {
      if (!menu.contains(e.target as Node)) {
        menu.remove();
        document.removeEventListener("click", dismiss);
      }
    };
    setTimeout(() => document.addEventListener("click", dismiss), 0);
  }

  private async showHPPicker(lNode: any, portName: string, event: MouseEvent): Promise<void> {
    const sid = this.state.sessionId;
    if (!sid) return;
    const { hyperparameters } = await graphs.getHyperparameters(sid);
    this.showBindingPicker(event, hyperparameters.map((h) => h.name), async (name) => {
      await graphs.updateNode(sid, lNode.zihuanId as string, {
        port_bindings: { [portName]: { kind: "Hyperparameter", name } },
      });
      await this.reloadCurrentSession();
    });
  }

  private async showVarPicker(lNode: any, portName: string, event: MouseEvent): Promise<void> {
    const sid = this.state.sessionId;
    if (!sid) return;
    const variables = await graphs.getVariables(sid);
    this.showBindingPicker(event, variables.map((v) => v.name), async (name) => {
      await graphs.updateNode(sid, lNode.zihuanId as string, {
        port_bindings: { [portName]: { kind: "Variable", name } },
      });
      await this.reloadCurrentSession();
    });
  }

  private showBindingPicker(
    event: MouseEvent,
    names: string[],
    onSelect: (name: string) => Promise<void>
  ): void {
    document.getElementById("zh-port-picker")?.remove();

    const picker = document.createElement("div");
    picker.id = "zh-port-picker";
    picker.style.cssText = `
      position:fixed;z-index:10001;
      left:${event.clientX + 8}px;top:${event.clientY}px;
      background:#1a1a2e;border:1px solid #2a2a4a;border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.6);
      font-family:sans-serif;font-size:13px;color:#e0e0e0;
      min-width:160px;max-height:220px;overflow-y:auto;
    `;

    if (names.length === 0) {
      const empty = document.createElement("div");
      empty.textContent = "(无可用项)";
      empty.style.cssText = "padding:8px 14px;color:#888;";
      picker.appendChild(empty);
    }

    for (const name of names) {
      const item = document.createElement("div");
      item.textContent = name;
      item.style.cssText = "padding:8px 14px;cursor:pointer;border-bottom:1px solid #1a2a4a;";
      item.addEventListener("mouseenter", () => { item.style.background = "#1a3a6e"; });
      item.addEventListener("mouseleave", () => { item.style.background = ""; });
      item.addEventListener("click", () => { picker.remove(); onSelect(name).catch(console.error); });
      picker.appendChild(item);
    }

    document.body.appendChild(picker);

    const dismiss = (e: MouseEvent) => {
      if (!picker.contains(e.target as Node)) {
        picker.remove();
        document.removeEventListener("click", dismiss);
      }
    };
    setTimeout(() => document.addEventListener("click", dismiss), 0);
  }

  private async clearPortBinding(lNode: any, portName: string): Promise<void> {
    const sid = this.state.sessionId;
    if (!sid) return;
    const def = await graphs.get(sid);
    const nodeIdx = def.nodes.findIndex((n) => n.id === (lNode.zihuanId as string));
    if (nodeIdx < 0) return;
    const newBindings = { ...def.nodes[nodeIdx].port_bindings };
    delete newBindings[portName];
    const updatedGraph = {
      ...def,
      nodes: def.nodes.map((n, i) =>
        i === nodeIdx ? { ...n, port_bindings: newBindings } : n
      ),
    };
    await graphs.put(sid, updatedGraph);
    await this.reloadCurrentSession();
  }
}

/** Find the registered LiteGraph type key for a backend type_id. */
function findRegisteredType(typeId: string): string | null {
  const nodeTypes = (LiteGraph as any).registered_node_types as Record<string, unknown>;
  for (const key of Object.keys(nodeTypes)) {
    const cls = nodeTypes[key] as any;
    if (cls.zihuanTypeId === typeId) return key;
  }
  return null;
}
