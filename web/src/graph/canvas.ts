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

  /** Called whenever the breadcrumb navigation path changes */
  onNavigationChange?: (labels: string[]) => void;

  constructor(canvasEl: HTMLCanvasElement) {
    this.lGraph = new (LGraph as any)();
    this.lCanvas = new (LGraphCanvas as any)(canvasEl, this.lGraph);

    // Wire up LiteGraph change callbacks
    this.lGraph.onAfterExecute = () => {};

    // Listen to node add/remove/connect events
    this.lGraph.onNodeAdded = (node: any) => this.onNodeAdded(node);
    this.lGraph.onNodeRemoved = (node: any) => this.onNodeRemoved(node);
    this.lGraph.onConnectionChange = (node: any) => this.onConnectionChanged(node);
  }

  get sessionId(): string | null {
    return this.state.sessionId;
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
