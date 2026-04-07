// Litegraph canvas wrapper — bridges LiteGraph and the Zihuan API

import { LGraph, LGraphCanvas, LiteGraph } from "@comfyorg/litegraph";
import { graphs } from "../api/client";
import type { NodeGraphDefinition, NodeDefinition, EdgeDefinition, NodeTypeInfo } from "../api/types";
import { setupNodeWidgets } from "./widgets";
import { portTypeString, getNodeTypeInfo } from "./registry";
import type { BrainToolDefinition, EmbeddedFunctionConfig } from "../ui/dialogs";
import { showNodeInfoDialog } from "../ui/dialogs";
import { getLiteGraphColors, getPortColor, onThemeChange, getBoundaryNodeColors } from "../ui/theme";

/** Title bar height constant (matches LiteGraph.NODE_TITLE_HEIGHT default). */
const NODE_TITLE_HEIGHT = 30;

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

  /** Called whenever the graph is modified by the user (e.g. node moved) */
  onGraphDirty?: () => void;

  /**
   * Called when the user right-clicks on an empty area of the canvas.
   * Receives the position in graph coordinates where the node should be placed.
   */
  onAddNodeRequest?: (graphX: number, graphY: number) => void;

  constructor(canvasEl: HTMLCanvasElement) {
    this.lGraph = new (LGraph as any)();
    this.lCanvas = new (LGraphCanvas as any)(canvasEl, this.lGraph);

    // Patch isValidConnection so that "Any" typed ports accept all connections.
    const _origIsValid = (LiteGraph as any).isValidConnection.bind(LiteGraph);
    (LiteGraph as any).isValidConnection = function (type_a: unknown, type_b: unknown): boolean {
      if (
        type_a === "Any" || type_b === "Any" ||
        (typeof type_a === "string" && type_a.toLowerCase() === "any") ||
        (typeof type_b === "string" && type_b.toLowerCase() === "any")
      ) return true;
      return _origIsValid(type_a, type_b);
    };

    // Apply pink-purple theme colours to LiteGraph, then subscribe to theme changes.
    this.applyLiteGraphTheme();
    onThemeChange(() => this.applyLiteGraphTheme());

    // Use orthogonal routing (STRAIGHT_LINK = 0): horizontal → vertical → horizontal
    (this.lCanvas as any).links_render_mode = 0;

    // Override drawNodeWidgets to draw binding badges on widget-linked slots
    // AFTER the widget backgrounds are rendered (so badges are visible on top).
    const origDrawNodeWidgets = (this.lCanvas as any).drawNodeWidgets.bind(this.lCanvas);
    (this.lCanvas as any).drawNodeWidgets = (node: any, posY: any, ctx: CanvasRenderingContext2D) => {
      // Temporarily mask password widget values so they render as bullets on canvas.
      const savedPasswordValues: Array<{ w: any; real: any }> = [];
      if (node.widgets) {
        for (const w of node.widgets as any[]) {
          if (w._isPassword) {
            savedPasswordValues.push({ w, real: w.value });
            w.value = "•".repeat(String(w.value ?? "").length || 8);
          }
        }
      }
      origDrawNodeWidgets(node, posY, ctx);
      // Restore real values immediately after drawing.
      for (const { w, real } of savedPasswordValues) w.value = real;
      drawWidgetBindingBadges.call(node, ctx);
    };

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
      this.onGraphDirty?.();
    };

    // Draw data-type labels at the connection midpoint.
    // If the midpoint falls inside any node's bounding box the label is shifted
    // upward until it clears the node's top edge.
    (this.lCanvas as any).onDrawForeground = (ctx: CanvasRenderingContext2D) => {
      const scale: number = (this.lCanvas as any).ds?.scale ?? 1;
      if (scale < 0.3) return; // skip labels when zoomed out too far
      const renderedPaths: Set<any> = (this.lCanvas as any).renderedPaths;
      if (!renderedPaths) return;
      const fontSize = Math.round(10 / scale);
      const titleH: number = (LiteGraph as any).NODE_TITLE_HEIGHT ?? 24;
      const allNodes: any[] = (this.lGraph as any)._nodes ?? [];
      ctx.save();
      ctx.font = `bold ${fontSize}px sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      // Deduplicate: one label per (origin_id, origin_slot) pair.
      const drawnPorts = new Set<string>();
      for (const seg of renderedPaths) {
        // Skip Reroute objects (they have no origin_id) and wildcard type
        if (seg.origin_id === undefined) continue;
        let typeName: string = String(seg.type ?? "");
        if (!typeName || typeName === "*" || typeName === "null" || typeName === "undefined") continue;
        // If the link type is "Any", resolve the concrete type through the graph
        if (typeName === "Any" && this.state.graph) {
          const originNode = this.lGraph.getNodeById(seg.origin_id) as any;
          if (originNode?.zihuanId) {
            const originDef = this.state.graph.nodes.find((n) => n.id === originNode.zihuanId);
            const port = originDef?.output_ports[seg.origin_slot];
            if (port) {
              typeName = resolveConcretePortType(this.state.graph, originNode.zihuanId, port.name, false);
            }
          }
        }
        const portKey = `${seg.origin_id}:${seg.origin_slot}`;
        if (drawnPorts.has(portKey)) continue;
        drawnPorts.add(portKey);
        const pos: Float32Array | number[] = seg._pos;
        if (!pos) continue;
        const x = pos[0];
        let y = pos[1];
        const padding = 3 / scale;
        const metrics = ctx.measureText(typeName);
        const pw = metrics.width + padding * 2;
        const ph = fontSize + padding * 2;
        // If the label center falls inside any node, push it above that node's top edge.
        for (const node of allNodes) {
          if (!node.pos || !node.size) continue;
          const nx = node.pos[0];
          const ny = node.pos[1] - titleH;
          const nw = node.size[0];
          const nh = node.size[1] + titleH;
          if (x > nx && x < nx + nw && y > ny && y < ny + nh) {
            y = ny - ph / 2 - 2 / scale;
          }
        }
        const rx = 3 / scale;
        // Pill background
        ctx.fillStyle = "rgba(0,0,0,0.65)";
        ctx.beginPath();
        (ctx as any).roundRect(x - pw / 2, y - ph / 2, pw, ph, rx);
        ctx.fill();
        // Label text
        ctx.fillStyle = "#ffffff";
        ctx.fillText(typeName, x, y);
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

        // Widget-linked input slots are not in getSlotInPosition's standard vertical
        // slot layout.  Fall back to getWidgetOnPos and map the widget to its input.
        const widget = (node as any).getWidgetOnPos(gx, gy, true);
        if (widget) {
          const inputs = (node as any).inputs as Array<{ name: string; widget?: { name: string } }> | undefined;
          if (inputs) {
            const idx = inputs.findIndex((inp) => inp.widget && inp.widget.name === widget.name);
            if (idx >= 0) {
              e.preventDefault();
              e.stopPropagation();
              this.showPortBindingMenu(node, idx, inputs[idx].name, e);
              return;
            }
          }
        }
      }

      // If right-clicking on a node (but not a slot), select it so the
      // context menu's "删除" action is enabled for that node.
      if (node) {
        const alreadySelected = !!(this.lCanvas as any).selected_nodes?.[node.id];
        if (!alreadySelected) {
          (this.lCanvas as any).selectNode(node, false);
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
   * Returns the root (top-level tab) session ID regardless of subgraph depth.
   * When inside a subgraph the virtual session is not in the tab list; this
   * getter always returns the session that corresponds to an open tab.
   */
  get rootSessionId(): string | null {
    if (this.subgraphStack.length > 0) {
      return this.subgraphStack[0].parentSessionId;
    }
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

    // Recolor boundary nodes to match the new theme
    const boundaryColors = getBoundaryNodeColors();
    const allNodes: any[] = (this.lGraph as any)._nodes ?? [];
    for (const node of allNodes) {
      if (node.type === "function_inputs" || node.type === "function_outputs") {
        node.color = boundaryColors.header;
        node.bgcolor = boundaryColors.bg;
      }
    }

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
    // color_off = type color if inline data provided, red if required+empty, gray otherwise
    if (node.inputs) {
      for (let i = 0; i < nodeDef.input_ports.length; i++) {
        const p = nodeDef.input_ports[i];
        const col = getPortColor(portTypeString(p.data_type as string | object));
        const hasInlineValue = nodeDef.inline_values != null && nodeDef.inline_values[p.name] != null;
        node.inputs[i].color_on  = col;
        node.inputs[i].color_off = hasInlineValue ? col : (p.required ? "#e74c3c" : "#555568");
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
          const dotColor = binding.kind === "hyperparameter" ? "#e67e22" : "#1abc9c";
          node.inputs[i].color_on = dotColor;
          node.inputs[i].color_off = dotColor;
        }
      }
    }
    // Draw colored badge pills next to each bound input's label,
    // and draw the help "?" button when the mouse is hovering the node.
    node._helpVisible = false;
    node.onMouseEnter = () => {
      node._helpVisible = true;
      this.lGraph.setDirtyCanvas(true, false);
    };
    node.onMouseLeave = () => {
      node._helpVisible = false;
      this.lGraph.setDirtyCanvas(true, false);
    };
    node.onDrawForeground = function(this: any, ctx: CanvasRenderingContext2D) {
      drawBindingBadges.call(this, ctx);
      drawHelpButton.call(this, ctx);
    };
    node.onMouseDown = (e: MouseEvent, pos: [number, number], _lCanvas: any): boolean | undefined => {
      const btnX = node.size[0] - 12;
      const btnY = -NODE_TITLE_HEIGHT / 2;
      const dx = pos[0] - btnX;
      const dy = pos[1] - btnY;
      if (Math.sqrt(dx * dx + dy * dy) <= 10) {
        e.preventDefault();
        e.stopPropagation();
        this.showNodeHelpDialog(node);
        return true;
      }
      return undefined;
    };

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

    // Special styling for function boundary nodes — teal header to distinguish them
    // from regular nodes, and mark them non-deletable.
    if (nodeDef.node_type === "function_inputs" || nodeDef.node_type === "function_outputs") {
      const boundaryColors = getBoundaryNodeColors();
      node.color = boundaryColors.header;
      node.bgcolor = boundaryColors.bg;
      node.block_delete = true;
    }

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
      const resolvedType = this.state.graph
        ? resolveConcretePortType(this.state.graph, originDef.id, port.name, false)
        : portTypeString(port.data_type as string | object);
      link.color = getPortColor(resolvedType);
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

    // Boundary nodes must never be deleted.
    if (nodeId === "__function_inputs__" || nodeId === "__function_outputs__") return;

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

    const selectedNodes: any[] = Object.values((this.lCanvas as any).selected_nodes ?? {});
    const hasSelection = selectedNodes.length > 0;
    const hasClipboard = this.nodeClipboard.length > 0;

    const menu = document.createElement("div");
    menu.id = "zh-canvas-menu";
    menu.style.cssText = `
      position:fixed;z-index:10000;
      left:${event.clientX}px;top:${event.clientY}px;
      background:var(--toolbar-bg);border:1px solid var(--border);border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.4);
      font-family:sans-serif;font-size:13px;color:var(--text);min-width:170px;overflow:hidden;
    `;

    const makeItem = (label: string, enabled: boolean, onClick: () => void) => {
      const item = document.createElement("div");
      item.textContent = label;
      item.style.cssText = `padding:8px 14px;cursor:${enabled ? "pointer" : "default"};border-bottom:1px solid var(--border);color:${enabled ? "var(--text)" : "var(--text-dim)"};`;
      if (enabled) {
        item.addEventListener("mouseenter", () => { item.style.background = "var(--node-hover)"; });
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

    const hasDeletable = selectedNodes.some((lNode: any) => {
      const nid: string | undefined = lNode.zihuanId;
      return nid !== "__function_inputs__" && nid !== "__function_outputs__";
    });
    makeItem("删除", hasDeletable, () => {
      this.deleteSelectedNodes().catch(console.error);
    });

    makeItem("提取为函数子图", hasSelection && this.state.graph !== null, () => {
      this.convertSelectionToFunction().catch(console.error);
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
    const selectedLNodes: any[] = Object.values((this.lCanvas as any).selected_nodes ?? {});
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
    const selectedLNodes: any[] = Object.values((this.lCanvas as any).selected_nodes ?? {});
    if (selectedLNodes.length === 0) return;
    // Remove each selected node via LiteGraph — onNodeRemoved will sync to backend.
    // Boundary nodes (function_inputs / function_outputs) are protected and skipped.
    for (const lNode of [...selectedLNodes]) {
      const nid: string | undefined = lNode.zihuanId;
      if (nid === "__function_inputs__" || nid === "__function_outputs__") continue;
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
    const hpSid = this.rootSessionId ?? sid;
    const { hyperparameters } = await graphs.getHyperparameters(hpSid);
    this.showBindingPicker(event, hyperparameters.map((h) => h.name), async (name) => {
      await graphs.updateNode(sid, lNode.zihuanId as string, {
        port_bindings: { [portName]: { kind: "hyperparameter", name } },
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
        port_bindings: { [portName]: { kind: "variable", name } },
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

  // ─── Node help dialog ─────────────────────────────────────────────────────

  /** Show the node info / help dialog for the given LiteGraph node. */
  private showNodeHelpDialog(lNode: any): void {
    const typeId: string = (lNode.constructor as any).zihuanTypeId ?? "";
    const typeInfo: NodeTypeInfo | undefined = getNodeTypeInfo(typeId);
    if (!typeInfo) return;

    const nodeId: string = lNode.zihuanId ?? "";
    const graph = this.state.graph;

    // Build per-port connection info from the graph edge list.
    const inputConns = typeInfo.input_ports.map((p) => {
      const connectedTo: Array<{ nodeName: string; portName: string }> = [];
      if (graph) {
        for (const edge of graph.edges) {
          if (edge.to_node_id === nodeId && edge.to_port === p.name) {
            const fromNode = graph.nodes.find((n) => n.id === edge.from_node_id);
            if (fromNode) connectedTo.push({ nodeName: fromNode.name, portName: edge.from_port });
          }
        }
      }
      return { portName: p.name, dataType: typeof p.data_type === "string" ? p.data_type : portTypeString(p.data_type), description: p.description, required: p.required, connectedTo };
    });

    const outputConns = typeInfo.output_ports.map((p) => {
      const connectedTo: Array<{ nodeName: string; portName: string }> = [];
      if (graph) {
        for (const edge of graph.edges) {
          if (edge.from_node_id === nodeId && edge.from_port === p.name) {
            const toNode = graph.nodes.find((n) => n.id === edge.to_node_id);
            if (toNode) connectedTo.push({ nodeName: toNode.name, portName: edge.to_port });
          }
        }
      }
      return { portName: p.name, dataType: typeof p.data_type === "string" ? p.data_type : portTypeString(p.data_type), description: p.description, required: p.required, connectedTo };
    });

    showNodeInfoDialog(typeInfo, inputConns, outputConns);
  }

  // ─── Convert selection to function subgraph ───────────────────────────────

  /**
   * Extract the currently selected nodes into a new `function` node.
   *
   * Algorithm:
   *  1. Collect selected node IDs.
   *  2. Classify every edge in the parent graph:
   *     - internal: both endpoints selected → copied verbatim into the subgraph
   *     - external_in: source outside selection, target inside → becomes a function input port
   *     - external_out: source inside selection, target outside → becomes a function output port
   *  3. Build the subgraph (selected nodes + __function_inputs__ / __function_outputs__ boundary nodes
   *     + internal edges + boundary-connection edges).
   *  4. Build the `function` NodeDefinition to replace the selection in the parent graph.
   *  5. Rewrite parent graph edges: keep edges that don't touch the selection, rewire
   *     external_in and external_out edges through the function node.
   *  6. PUT the updated graph and reload.
   */
  private async convertSelectionToFunction(): Promise<void> {
    const sid = this.state.sessionId;
    const graph = this.state.graph;
    if (!sid || !graph) return;

    // 1. Collect selected backend node IDs
    const selectedLNodes: any[] = Object.values((this.lCanvas as any).selected_nodes ?? {});
    const selectedIds = new Set<string>();
    for (const lNode of selectedLNodes) {
      const id = lNode.zihuanId as string | undefined;
      if (id) selectedIds.add(id);
    }
    if (selectedIds.size === 0) return;

    // 2. Classify edges
    type ExtInEdge  = { edge: EdgeDefinition; fnPortName: string };
    type ExtOutEdge = { edge: EdgeDefinition; fnPortName: string };

    const internalEdges: EdgeDefinition[] = [];
    const externalInEdges: ExtInEdge[]    = [];  // outside → inside
    const externalOutEdges: ExtOutEdge[]  = [];  // inside → outside

    // Track used port names to avoid collisions
    const usedInPortNames  = new Set<string>();
    // For deduplication of output ports: (fromNode, fromPort) → fnPortName
    const outPortKeyToName = new Map<string, string>();

    const safePortName = (preferred: string, usedSet: Set<string>, nodeName: string, portName: string): string => {
      if (!usedSet.has(preferred)) return preferred;
      const alt = `${nodeName.replace(/[^a-zA-Z0-9]+/g, "_")}_${portName}`;
      if (!usedSet.has(alt)) return alt;
      // Last resort: append a counter
      let i = 2;
      while (usedSet.has(`${alt}_${i}`)) i++;
      return `${alt}_${i}`;
    };

    // Helper: get Port data_type string for a given node+port
    const getPortDataType = (nodeId: string, portName: string, isOutput: boolean): string => {
      const nodeDef = graph.nodes.find(n => n.id === nodeId);
      if (!nodeDef) return "Any";
      const ports = isOutput ? nodeDef.output_ports : nodeDef.input_ports;
      const port = ports.find(p => p.name === portName);
      if (!port) return "Any";
      return typeof port.data_type === "string" ? port.data_type : portTypeString(port.data_type as object);
    };

    const getNodeDisplayName = (nodeId: string): string => {
      return graph.nodes.find(n => n.id === nodeId)?.name ?? nodeId;
    };

    for (const edge of graph.edges) {
      const fromSel = selectedIds.has(edge.from_node_id);
      const toSel   = selectedIds.has(edge.to_node_id);

      if (fromSel && toSel) {
        internalEdges.push(edge);
      } else if (!fromSel && toSel) {
        // external_in: one port per edge (inputs have at most one incoming edge)
        const preferred  = edge.to_port;
        const nodeName   = getNodeDisplayName(edge.to_node_id);
        const fnPortName = safePortName(preferred, usedInPortNames, nodeName, edge.to_port);
        usedInPortNames.add(fnPortName);
        externalInEdges.push({ edge, fnPortName });
      } else if (fromSel && !toSel) {
        // external_out: deduplicated by (from_node, from_port)
        const key = `${edge.from_node_id}::${edge.from_port}`;
        if (!outPortKeyToName.has(key)) {
          const usedOut    = new Set(outPortKeyToName.values());
          const preferred  = edge.from_port;
          const nodeName   = getNodeDisplayName(edge.from_node_id);
          const fnPortName = safePortName(preferred, usedOut, nodeName, edge.from_port);
          outPortKeyToName.set(key, fnPortName);
        }
        externalOutEdges.push({ edge, fnPortName: outPortKeyToName.get(key)! });
      }
      // else: both outside — kept in parent graph unchanged
    }

    // 3. Build function signature port defs
    const fnInputPortDefs: Array<{ name: string; data_type: string }> = externalInEdges.map(ei => ({
      name: ei.fnPortName,
      data_type: getPortDataType(ei.edge.from_node_id, ei.edge.from_port, true),
    }));

    // Unique output defs (one per fnPortName)
    const seenOutNames = new Set<string>();
    const fnOutputPortDefs: Array<{ name: string; data_type: string }> = [];
    for (const eo of externalOutEdges) {
      if (!seenOutNames.has(eo.fnPortName)) {
        seenOutNames.add(eo.fnPortName);
        fnOutputPortDefs.push({
          name: eo.fnPortName,
          data_type: getPortDataType(eo.edge.from_node_id, eo.edge.from_port, true),
        });
      }
    }

    // 4. Compute centroid of selected nodes for function node placement
    const selectedDefs = graph.nodes.filter(n => selectedIds.has(n.id));
    const xs = selectedDefs.map(n => n.position?.x ?? 0);
    const ys = selectedDefs.map(n => n.position?.y ?? 0);
    const centroidX = xs.reduce((a, b) => a + b, 0) / (xs.length || 1);
    const centroidY = ys.reduce((a, b) => a + b, 0) / (ys.length || 1);

    // Boundary node X positions (relative to selected bounding box)
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    const inputsBoundaryX  = minX - 300;
    const outputsBoundaryX = maxX + 300;
    const boundaryY        = centroidY;

    // 5. Build __function_inputs__ boundary node
    const fnInputsNodeInlineValues: Record<string, unknown> = {
      signature: fnInputPortDefs,
    };
    const fnInputsNode: NodeDefinition = {
      id: "__function_inputs__",
      name: "函数输入",
      description: "函数子图的输入边界节点",
      node_type: "function_inputs",
      input_ports: [
        { name: "signature",       data_type: "Json", description: null, required: false },
        { name: "runtime_values",  data_type: "Json", description: null, required: false },
      ],
      output_ports: fnInputPortDefs.map(p => ({
        name: p.name,
        data_type: p.data_type,
        description: null,
        required: false,
      })),
      dynamic_input_ports: false,
      dynamic_output_ports: true,
      position: { x: inputsBoundaryX, y: boundaryY },
      size: { width: 220, height: 120 },
      inline_values: fnInputsNodeInlineValues,
      port_bindings: {},
      has_error: false,
      has_cycle: false,
    };

    // 6. Build __function_outputs__ boundary node
    const fnOutputsNodeInlineValues: Record<string, unknown> = {
      signature: fnOutputPortDefs,
    };
    const fnOutputsInputPorts: Array<{ name: string; data_type: string; description: null; required: boolean }> = [
      { name: "signature", data_type: "Json", description: null, required: false },
      ...fnOutputPortDefs.map(p => ({ name: p.name, data_type: p.data_type, description: null, required: false })),
    ];
    const fnOutputsNode: NodeDefinition = {
      id: "__function_outputs__",
      name: "函数输出",
      description: "函数子图的输出边界节点",
      node_type: "function_outputs",
      input_ports: fnOutputsInputPorts,
      output_ports: [],
      dynamic_input_ports: true,
      dynamic_output_ports: false,
      position: { x: outputsBoundaryX, y: boundaryY },
      size: { width: 220, height: 120 },
      inline_values: fnOutputsNodeInlineValues,
      port_bindings: {},
      has_error: false,
      has_cycle: false,
    };

    // 7. Build subgraph edges
    //    a) internal edges (verbatim)
    //    b) __function_inputs__ output → selected node input (one per externalInEdge)
    //    c) selected node output → __function_outputs__ input (one per unique externalOutEdge key)
    const subgraphEdges: EdgeDefinition[] = [
      ...internalEdges,
      ...externalInEdges.map(ei => ({
        from_node_id: "__function_inputs__",
        from_port: ei.fnPortName,
        to_node_id: ei.edge.to_node_id,
        to_port: ei.edge.to_port,
      })),
    ];
    // Deduplicate output edges by (from_node, from_port) — already done via outPortKeyToName
    const addedOutKeys = new Set<string>();
    for (const eo of externalOutEdges) {
      const key = `${eo.edge.from_node_id}::${eo.edge.from_port}`;
      if (!addedOutKeys.has(key)) {
        addedOutKeys.add(key);
        subgraphEdges.push({
          from_node_id: eo.edge.from_node_id,
          from_port:    eo.edge.from_port,
          to_node_id:   "__function_outputs__",
          to_port:      eo.fnPortName,
        });
      }
    }

    // 8. Build EmbeddedFunctionConfig (matches Rust EmbeddedFunctionConfig / dialogs.ts)
    const functionConfig = {
      name:        "New Function",
      description: "",
      inputs:  fnInputPortDefs,
      outputs: fnOutputPortDefs,
      subgraph: {
        nodes: [...selectedDefs, fnInputsNode, fnOutputsNode],
        edges: subgraphEdges,
        hyperparameter_groups: [],
        hyperparameters: [],
        variables: [],
        metadata: { name: null, description: null, version: null },
      } as NodeGraphDefinition,
    };

    // 9. Build the replacement function NodeDefinition in the parent graph
    const fnNodeId = crypto.randomUUID();
    const functionNode: NodeDefinition = {
      id: fnNodeId,
      name: "New Function",
      description: null,
      node_type: "function",
      input_ports: fnInputPortDefs.map(p => ({
        name: p.name,
        data_type: p.data_type,
        description: null,
        required: false,
      })),
      output_ports: fnOutputPortDefs.map(p => ({
        name: p.name,
        data_type: p.data_type,
        description: null,
        required: false,
      })),
      dynamic_input_ports: true,
      dynamic_output_ports: true,
      position: { x: centroidX, y: centroidY },
      size: { width: 220, height: 80 + Math.max(fnInputPortDefs.length, fnOutputPortDefs.length) * 20 },
      inline_values: { function_config: functionConfig as unknown as Record<string, unknown> },
      port_bindings: {},
      has_error: false,
      has_cycle: false,
    };

    // 10. Build updated parent graph
    const newNodes: NodeDefinition[] = [
      ...graph.nodes.filter(n => !selectedIds.has(n.id)),
      functionNode,
    ];

    // Keep edges that don't touch the selection at all
    const keptEdges: EdgeDefinition[] = graph.edges.filter(
      e => !selectedIds.has(e.from_node_id) && !selectedIds.has(e.to_node_id)
    );

    // Rewire external_in: original source → function node's matching input port
    const rewiredIn: EdgeDefinition[] = externalInEdges.map(ei => ({
      from_node_id: ei.edge.from_node_id,
      from_port:    ei.edge.from_port,
      to_node_id:   fnNodeId,
      to_port:      ei.fnPortName,
    }));

    // Rewire external_out: function node's output port → all original targets (fan-out)
    const rewiredOut: EdgeDefinition[] = externalOutEdges.map(eo => ({
      from_node_id: fnNodeId,
      from_port:    eo.fnPortName,
      to_node_id:   eo.edge.to_node_id,
      to_port:      eo.edge.to_port,
    }));

    const newGraph: NodeGraphDefinition = {
      ...graph,
      nodes: newNodes,
      edges: [...keptEdges, ...rewiredIn, ...rewiredOut],
    };

    // 11. Persist and reload
    await graphs.put(sid, newGraph);
    await this.reloadCurrentSession();
    this.state.dirty = true;
  }
}

/** Draw the "?" help button in the top-right of the node title bar when hovered. */
function drawHelpButton(this: any, ctx: CanvasRenderingContext2D): void {
  if (!this._helpVisible) return;
  // In LiteGraph's local node coordinate system, the title bar spans y ∈ [-NODE_TITLE_HEIGHT, 0]
  // and body spans y ≥ 0. onDrawForeground is called with origin at node body top-left, so
  // title bar center is at y = -NODE_TITLE_HEIGHT / 2.
  const cx = (this.size[0] as number) - 14;
  const cy = -NODE_TITLE_HEIGHT / 2;
  const r = 8;

  ctx.save();
  // Circle background
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.fillStyle = "rgba(255,255,255,0.18)";
  ctx.fill();
  ctx.strokeStyle = "rgba(255,255,255,0.55)";
  ctx.lineWidth = 1;
  ctx.stroke();
  // "?" text
  ctx.font = "bold 11px sans-serif";
  ctx.fillStyle = "#ffffff";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("?", cx, cy + 0.5);
  ctx.restore();
}

/** Draw colored badge pills to the right of each bound input port's label text.
 *  Only handles NON-widget-linked slots (drawn before widgets via onDrawForeground). */
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
    // Widget input slots (linked to widgets) are not drawn as vertical slots —
    // their badges are handled by drawWidgetBindingBadges (runs after widgets).
    if (input.pos || (this.widgets?.length && input.widget)) continue;
    verticalSlotIndex++;

    const portName: string = input.name;
    const binding = bindings[portName];
    if (!binding) continue;

    const localY = (verticalSlotIndex + 0.7) * SLOT_HEIGHT + ((<any>this.constructor).slot_start_y || 0);
    drawBadgePill(ctx, portName, binding, LABEL_X, localY + LABEL_BASELINE_OFFSET, FONT_SIZE);
  }

  ctx.restore();
}

/** Draw binding badges for widget-linked input slots.
 *  Called AFTER drawNodeWidgets so badges render on top of widget backgrounds. */
function drawWidgetBindingBadges(
  this: any,
  ctx: CanvasRenderingContext2D
): void {
  const bindings: Record<string, { kind: string; name: string }> = this._portBindings;
  if (!bindings || !this.inputs || !this.widgets) return;

  const FONT_SIZE = 12;
  const FONT = "Arial";
  const WIDGET_HEIGHT: number = (LiteGraph as any).NODE_WIDGET_HEIGHT || 20;
  const MARGIN: number = 15; // BaseWidget.margin
  const LABEL_X = MARGIN * 2; // widget label starts at margin*2

  ctx.save();
  ctx.font = `normal ${FONT_SIZE}px ${FONT}`;
  ctx.textBaseline = "middle";

  for (let i = 0; i < this.inputs.length; i++) {
    const input = this.inputs[i];
    if (!input.widget) continue;

    const binding = bindings[input.name];
    if (!binding) continue;

    const widgetName: string = typeof input.widget === "object" ? input.widget.name : input.widget;
    const widget = (this.widgets as any[]).find((w: any) => w.name === widgetName);
    if (!widget) continue;

    // widget.last_y is set by drawWidgets (just ran); widget.y is set by arrange().
    const wy: number = widget.last_y ?? widget.y ?? 0;
    const centerY = wy + WIDGET_HEIGHT * 0.5;

    drawBadgePill(ctx, input.name, binding, LABEL_X, centerY, FONT_SIZE);
  }

  ctx.restore();
}

/** Shared helper: draw a single binding badge pill at the given position. */
function drawBadgePill(
  ctx: CanvasRenderingContext2D,
  portName: string,
  binding: { kind: string; name: string },
  labelX: number,
  centerY: number,
  fontSize: number
): void {
  const labelWidth = ctx.measureText(portName).width;
  const badgeText = (binding.kind === "hyperparameter" ? "\u2191" : "\u27f2") + binding.name;
  const badgePadX = 4;
  const badgePadY = 2;
  const badgeTextW = ctx.measureText(badgeText).width;
  const badgeH = fontSize + badgePadY * 2;
  const badgeW = badgeTextW + badgePadX * 2;
  const badgeX = labelX + labelWidth + 4;
  const badgeY = centerY - badgeH / 2;
  const badgeRadius = 3;

  const bgColor = binding.kind === "hyperparameter" ? "#e67e22" : "#1abc9c";
  ctx.fillStyle = bgColor;
  ctx.beginPath();
  (ctx as any).roundRect(badgeX, badgeY, badgeW, badgeH, badgeRadius);
  ctx.fill();

  ctx.fillStyle = "#ffffff";
  ctx.fillText(badgeText, badgeX + badgePadX, badgeY + badgeH / 2);
}

/**
 * Resolve the concrete DataType string for an output port, tracing through
 * "Any"-typed passthrough nodes (switch_gate, boolean_branch, etc.) by
 * following the graph edge definition backward.
 */
function resolveConcretePortType(
  graph: NodeGraphDefinition,
  nodeId: string,
  portName: string,
  isInput: boolean,
  visited = new Set<string>(),
): string {
  const key = `${nodeId}:${isInput ? "in" : "out"}:${portName}`;
  if (visited.has(key)) return "Any";
  visited.add(key);

  const nodeDef = graph.nodes.find((n) => n.id === nodeId);
  if (!nodeDef) return "Any";

  const ports = isInput ? nodeDef.input_ports : nodeDef.output_ports;
  const port = ports.find((p) => p.name === portName);
  if (!port) return "Any";

  const dt = typeof port.data_type === "string" ? port.data_type : portTypeString(port.data_type);
  if (dt !== "Any") return dt;

  if (isInput) {
    // Trace upstream: find the edge that feeds this input
    const edge = graph.edges.find((e) => e.to_node_id === nodeId && e.to_port === portName);
    if (edge) return resolveConcretePortType(graph, edge.from_node_id, edge.from_port, false, visited);
    return "Any";
  }

  // Output port is Any: try to resolve through any Any-typed input ports
  for (const inp of nodeDef.input_ports) {
    const inDt = typeof inp.data_type === "string" ? inp.data_type : portTypeString(inp.data_type);
    if (inDt !== "Any") continue;
    const resolved = resolveConcretePortType(graph, nodeId, inp.name, true, visited);
    if (resolved !== "Any") return resolved;
  }
  return "Any";
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
