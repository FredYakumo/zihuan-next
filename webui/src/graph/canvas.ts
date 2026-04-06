// Litegraph canvas wrapper — bridges LiteGraph and the Zihuan API

import { LGraph, LGraphCanvas, LiteGraph } from "@comfyorg/litegraph";
import { graphs } from "../api/client";
import type { NodeGraphDefinition, NodeDefinition, EdgeDefinition } from "../api/types";
import { setupNodeWidgets } from "./widgets";
import { portTypeString } from "./registry";
import type { BrainToolDefinition, EmbeddedFunctionConfig } from "../ui/dialogs";
import { getLiteGraphColors, getPortColor, onThemeChange } from "../ui/theme";

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

    // Apply pink-purple theme colours to LiteGraph, then subscribe to theme changes.
    this.applyLiteGraphTheme();
    onThemeChange(() => this.applyLiteGraphTheme());

    // Use orthogonal routing (STRAIGHT_LINK = 0): horizontal → vertical → horizontal
    (this.lCanvas as any).links_render_mode = 0;

    // Snap nodes to grid only on release, not during drag (avoids jitter and unnecessary work)
    LiteGraph.alwaysSnapToGrid = false;
    LiteGraph.CANVAS_GRID_SIZE = 10;

    // Snap selected nodes to grid after each drag ends
    (this.lCanvas as any).onNodeMoved = (_node: any) => {
      const selected: Set<any> | undefined = (this.lCanvas as any).selectedItems;
      if (selected?.size) {
        this.lGraph.snapToGrid(selected);
        this.lGraph.setDirtyCanvas(true, true);
      }
    };

    // Draw data-type labels at the midpoint of each rendered connection
    (this.lCanvas as any).onDrawForeground = (ctx: CanvasRenderingContext2D) => {
      const scale: number = (this.lCanvas as any).ds?.scale ?? 1;
      if (scale < 0.3) return; // skip labels when zoomed out too far
      const renderedPaths: Set<any> = (this.lCanvas as any).renderedPaths;
      if (!renderedPaths) return;
      const fontSize = Math.round(10 / scale);
      ctx.save();
      ctx.font = `bold ${fontSize}px sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      for (const seg of renderedPaths) {
        // Skip Reroute objects (they have no origin_id) and wildcard type
        if (seg.origin_id === undefined) continue;
        const typeName: string = String(seg.type ?? "");
        if (!typeName || typeName === "*" || typeName === "null" || typeName === "undefined") continue;
        const pos: Float32Array | number[] = seg._pos;
        if (!pos) continue;
        const x = pos[0];
        const y = pos[1];
        const padding = 3 / scale;
        const metrics = ctx.measureText(typeName);
        const tw = metrics.width;
        const th = fontSize;
        const pw = tw + padding * 2;
        const ph = th + padding * 2;
        const rx = 3 / scale;
        // Pill background
        ctx.fillStyle = "rgba(0,0,0,0.65)";
        ctx.beginPath();
        (ctx as any).roundRect(x - pw / 2, y - ph / 2 - fontSize * 0.8, pw, ph, rx);
        ctx.fill();
        // Label text
        ctx.fillStyle = "#ffffff";
        ctx.fillText(typeName, x, y - fontSize * 0.8);
      }
      ctx.restore();
    };

    // Wire up LiteGraph change callbacks
    this.lGraph.onAfterExecute = () => {};

    // Listen to node add/remove/connect events
    this.lGraph.onNodeAdded = (node: any) => this.onNodeAdded(node);
    this.lGraph.onNodeRemoved = (node: any) => this.onNodeRemoved(node);
    this.lGraph.onConnectionChange = (node: any) => this.onConnectionChanged(node);

    // Right-click context menu (capture phase so we preempt LiteGraph's own handler).
    canvasEl.addEventListener("contextmenu", (e: MouseEvent) => {
      // graph_mouse is updated by LiteGraph on every mousemove and holds graph-space coords.
      // convertEventToCanvasOffset(e) also produces graph-space coords directly (despite its name).
      const [gx, gy] = (this.lCanvas as any).convertEventToCanvasOffset(e) as [number, number];
      const node = this.lGraph.getNodeOnPos(gx, gy);

      // If right-clicking on a node's input slot, show port-binding menu instead.
      // getSlotInPosition expects graph-space coords (calls getInputPos which returns graph coords).
      if (node) {
        const found = (node as any).getSlotInPosition(gx, gy) as
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

  /** Apply theme-aware colour tokens to LiteGraph global and canvas-instance settings. */
  private applyLiteGraphTheme(): void {
    const c = getLiteGraphColors();

    // Global LiteGraph colour tokens
    (LiteGraph as any).NODE_DEFAULT_COLOR      = c.nodeHeader;
    (LiteGraph as any).NODE_DEFAULT_BGCOLOR    = c.nodeBg;
    (LiteGraph as any).NODE_DEFAULT_BOXCOLOR   = c.nodeBox;
    (LiteGraph as any).NODE_BOX_OUTLINE_COLOR  = c.nodeBoxOutline;
    (LiteGraph as any).NODE_TITLE_COLOR        = c.nodeTitleText;
    (LiteGraph as any).NODE_SELECTED_TITLE_COLOR = c.nodeSelectedTitle;
    (LiteGraph as any).NODE_TEXT_COLOR         = c.nodeText;
    (LiteGraph as any).NODE_TEXT_HIGHLIGHT_COLOR = c.nodeSelectedTitle;
    (LiteGraph as any).DEFAULT_SHADOW_COLOR    = c.shadow;
    (LiteGraph as any).WIDGET_BGCOLOR          = c.widgetBg;
    (LiteGraph as any).WIDGET_OUTLINE_COLOR    = c.widgetOutline;
    (LiteGraph as any).WIDGET_TEXT_COLOR       = c.widgetText;
    (LiteGraph as any).WIDGET_SECONDARY_TEXT_COLOR = c.widgetSecondary;
    (LiteGraph as any).WIDGET_DISABLED_TEXT_COLOR  = c.widgetDisabled;
    (LiteGraph as any).LINK_COLOR              = c.linkColor;
    (LiteGraph as any).EVENT_LINK_COLOR        = c.eventLinkColor;
    (LiteGraph as any).CONNECTING_LINK_COLOR   = c.connectingLinkColor;

    // Per-DataType wire colours — mutate in-place so descriptor constraints don't block us
    const tc = c.linkTypeColors;
    const primitives = ["String", "Integer", "Float", "Boolean", "Binary", "Password"];
    const complexTypes = ["Json", "MessageEvent", "OpenAIMessage", "QQMessage", "FunctionTools", "LLModel"];
    const refs = ["BotAdapterRef", "RedisRef", "MySqlRef", "TavilyRef", "SessionStateRef",
                  "OpenAIMessageSessionCacheRef", "LoopControlRef"];
    const typeColorMap: Record<string, string> = { Any: tc.any };
    for (const t of primitives)   typeColorMap[t] = tc.primitive;
    for (const t of complexTypes) typeColorMap[t] = tc.complex;
    for (const t of refs)         typeColorMap[t] = tc.ref;
    // Vec variants — both Debug format "Vec(T)" and portTypeString format "Vec<T>"
    for (const t of [...primitives, ...complexTypes, ...refs, "Any"]) {
      typeColorMap[`Vec(${t})`] = tc.array;
      typeColorMap[`Vec<${t}>`] = tc.array;
    }
    const ltc = (LiteGraph as any).link_type_colors as Record<string, string> | null | undefined;
    if (ltc) {
      // Mutate in-place so any descriptor constraints are respected
      for (const key of Object.keys(ltc)) delete ltc[key];
      Object.assign(ltc, typeColorMap);
    } else {
      (LiteGraph as any).link_type_colors = typeColorMap;
    }

    // Canvas-instance settings
    (this.lCanvas as any).clear_background_color = c.canvasBg;
    (this.lCanvas as any).node_title_color       = c.nodeTitleText;
    (this.lCanvas as any).default_link_color     = c.linkColor;

    // Generate a tiled dot-grid background image
    const tile = document.createElement("canvas");
    tile.width = 10; tile.height = 10;
    const tCtx = tile.getContext("2d")!;
    tCtx.fillStyle = c.canvasBg;
    tCtx.fillRect(0, 0, 10, 10);
    tCtx.fillStyle = c.gridDotColor;
    tCtx.beginPath();
    tCtx.arc(1, 1, 0.9, 0, Math.PI * 2);
    tCtx.fill();
    (this.lCanvas as any).background_image = tile.toDataURL("image/png");

    this.lGraph.setDirtyCanvas(true, true);
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

    this.colorizeAllLinks();
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

    // Apply type-based port dot colours for all slots.
    // color_on  = type color (shown when connected)
    // color_off = red if required and unconnected, gray otherwise
    if (node.inputs) {
      for (let i = 0; i < nodeDef.input_ports.length; i++) {
        const p = nodeDef.input_ports[i];
        const col = getPortColor(portTypeString(p.data_type as string | object));
        node.inputs[i].color_on  = col;
        node.inputs[i].color_off = p.required ? "#e74c3c" : "#555568";
      }
    }
    if (node.outputs) {
      for (let i = 0; i < nodeDef.output_ports.length; i++) {
        const p = nodeDef.output_ports[i];
        const col = getPortColor(portTypeString(p.data_type as string | object));
        node.outputs[i].color_on  = col;
        node.outputs[i].color_off = "#555568";
      }
    }

    // Visual indicator for bound ports: colored slot dot.
    const portBindings = nodeDef.port_bindings ?? {};
    node._portBindings = portBindings;
    if (node.inputs) {
      for (let i = 0; i < nodeDef.input_ports.length; i++) {
        const portName = nodeDef.input_ports[i].name;
        const binding = portBindings[portName];
        if (binding) {
          // Color the connector dot to signal the binding visually.
          const dotColor = binding.kind === "Hyperparameter" ? "#e67e22" : "#1abc9c";
          node.inputs[i].color_on = dotColor;
          node.inputs[i].color_off = dotColor;
        }
      }
    }
    // Draw colored badge pills next to each bound input's label.
    node.onDrawForeground = drawBindingBadges;

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

  /** Set link.color directly on every link based on the origin port's DataType. */
  private colorizeAllLinks(): void {
    const links = this.lGraph.links;
    if (!links) return;
    const linkList = Object.values(links) as any[];
    for (const link of linkList) {
      if (!link) continue;
      const originNode = this.lGraph.getNodeById(link.origin_id) as any;
      if (!originNode) continue;
      const originDef = this.state.graph?.nodes.find((n) => n.id === originNode.zihuanId);
      if (!originDef) continue;
      const port = originDef.output_ports[link.origin_slot];
      if (!port) continue;
      link.color = getPortColor(portTypeString(port.data_type as string | object));
    }
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
      this.colorizeAllLinks();
      this.lGraph.setDirtyCanvas(true, false);
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

  /** Exit subgraphs until the stack depth equals targetDepth (0 = root). */
  async exitSubgraphToDepth(targetDepth: number): Promise<void> {
    while (this.subgraphStack.length > targetDepth) {
      await this.exitSubgraph();
    }
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

/** Draw colored badge pills to the right of each bound input port's label text. */
function drawBindingBadges(
  this: any,
  ctx: CanvasRenderingContext2D
): void {
  const bindings: Record<string, { kind: string; name: string }> = this._portBindings;
  if (!bindings || !this.inputs) return;

  const SLOT_HEIGHT = 20; // LiteGraph.NODE_SLOT_HEIGHT
  const FONT_SIZE = 12;   // LiteGraph.NODE_SUBTEXT_SIZE
  const FONT = "Arial";   // LiteGraph.NODE_FONT
  const SLOT_DOT_X = SLOT_HEIGHT * 0.5; // dot is at (NODE_SLOT_HEIGHT/2, ...)
  const LABEL_X = SLOT_DOT_X + 10;     // label text starts here (matches LiteGraph slot draw)
  const LABEL_BASELINE_OFFSET = 5;     // ctx.fillText(..., pos[0]+10, pos[1]+5)

  ctx.save();
  ctx.font = `normal ${FONT_SIZE}px ${FONT}`;
  ctx.textBaseline = "middle";

  // Count non-widget vertical inputs to find slot Y position (mirrors getInputSlotPos logic).
  let verticalSlotIndex = -1;
  for (let i = 0; i < this.inputs.length; i++) {
    const input = this.inputs[i];
    // Widget input slots (linked to widgets) are not drawn as vertical slots.
    if (input.pos || (this.widgets?.length && input.widget)) continue;
    verticalSlotIndex++;

    const portName: string = input.name;
    const binding = bindings[portName];
    if (!binding) continue;

    const localY = (verticalSlotIndex + 0.7) * SLOT_HEIGHT + ((<any>this.constructor).slot_start_y || 0);
    const labelText = portName;
    const labelWidth = ctx.measureText(labelText).width;

    const badgeText = (binding.kind === "Hyperparameter" ? "\u2191" : "\u27f2") + binding.name;
    const badgePadX = 4;
    const badgePadY = 2;
    const badgeTextMetrics = ctx.measureText(badgeText);
    const badgeTextW = badgeTextMetrics.width;
    const badgeH = FONT_SIZE + badgePadY * 2;
    const badgeW = badgeTextW + badgePadX * 2;
    const badgeX = LABEL_X + labelWidth + 4;
    const badgeY = localY + LABEL_BASELINE_OFFSET - badgeH / 2;
    const badgeRadius = 3;

    const bgColor = binding.kind === "Hyperparameter" ? "#e67e22" : "#1abc9c";
    ctx.fillStyle = bgColor;
    ctx.beginPath();
    (ctx as any).roundRect(badgeX, badgeY, badgeW, badgeH, badgeRadius);
    ctx.fill();

    ctx.fillStyle = "#ffffff";
    ctx.fillText(badgeText, badgeX + badgePadX, badgeY + badgeH / 2);
  }

  ctx.restore();
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
