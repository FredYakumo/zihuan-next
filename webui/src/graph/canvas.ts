import { LGraph, LGraphCanvas, LiteGraph } from "litegraph.js";
import type { NodeGraphDefinition, NodeDefinition, NodeTypeInfo } from "../api/types";
import type { BrainToolDefinition, EmbeddedFunctionConfig } from "../ui/dialogs/types";
import { HistoryManager } from "./history";
import { installLiteGraphPatches } from "./canvas/patches";
import { bindCanvasRendering, bindThemeLifecycle } from "./canvas/rendering";
import { CanvasGraphOps } from "./canvas/graph_ops";
import { CanvasInteractions } from "./canvas/interactions";
import { CanvasSubgraphController } from "./canvas/subgraphs";
import type { CanvasFacade, CanvasState, SubgraphStackEntry } from "./canvas/types";

export class ZihuanCanvas implements CanvasFacade {
  lGraph: InstanceType<typeof LGraph>;
  lCanvas: InstanceType<typeof LGraphCanvas>;
  state: CanvasState = { sessionId: null, graph: null, dirty: false };
  nodeMap = new Map<string, any>();
  subgraphStack: SubgraphStackEntry[] = [];
  nodeClipboard: NodeDefinition[] = [];
  _rebuilding = false;
  history = new HistoryManager<NodeGraphDefinition>();
  _widgetMutationTimer: ReturnType<typeof setTimeout> | null = null;
  _nodeMoveTimer: ReturnType<typeof setTimeout> | null = null;
  _pendingWidgetMutations = new Set<Promise<unknown>>();
  nodeTypes: NodeTypeInfo[] = [];

  onNavigationChange?: (labels: string[]) => void;
  onGraphDirty?: () => void;
  onHistoryChange?: () => void;
  onAddNodeRequest?: (graphX: number, graphY: number) => void;

  private readonly graphOps: CanvasGraphOps;
  private readonly interactions: CanvasInteractions;
  private readonly subgraphs: CanvasSubgraphController;

  constructor(canvasEl: HTMLCanvasElement) {
    this.lGraph = new (LGraph as any)();
    this.lCanvas = new (LGraphCanvas as any)(canvasEl, this.lGraph);

    installLiteGraphPatches();

    (this.lCanvas as any).processContextMenu = () => {};
    (this.lCanvas as any).links_render_mode = (LiteGraph as any).SPLINE_LINK ?? 2;

    this.graphOps = new CanvasGraphOps(this);
    this.interactions = new CanvasInteractions(this);
    this.subgraphs = new CanvasSubgraphController(this);

    bindThemeLifecycle(this);
    bindCanvasRendering(this, () => this.graphOps.onNodesMoved());
    this.graphOps.bindGraphEvents();
    this.interactions.bind();
  }

  get sessionId(): string | null {
    return this.state.sessionId;
  }

  get rootSessionId(): string | null {
    if (this.subgraphStack.length > 0) {
      return this.subgraphStack[0].parentSessionId;
    }
    return this.state.sessionId;
  }

  get isInSubgraph(): boolean {
    return this.subgraphStack.length > 0;
  }

  getCanvasViewport(): { offset: [number, number]; scale: number } | null {
    const ds = (this.lCanvas as any).ds as { offset: [number, number]; scale: number } | undefined;
    if (!ds) return null;
    return { offset: [...ds.offset] as [number, number], scale: ds.scale };
  }

  setCanvasViewport(offset: [number, number], scale: number): void {
    const ds = (this.lCanvas as any).ds;
    if (!ds) return;
    ds.offset = [...offset];
    ds.scale = scale;
    this.lGraph.setDirtyCanvas(true, true);
  }

  graphCenterPos(): { x: number; y: number } {
    const ds = (this.lCanvas as any).ds as { offset: [number, number]; scale: number } | undefined;
    const canvasEl = (this.lCanvas as any).canvas as HTMLCanvasElement | undefined;
    if (!ds || !canvasEl) return { x: 100, y: 100 };
    const width = canvasEl.width / (window.devicePixelRatio || 1);
    const height = canvasEl.height / (window.devicePixelRatio || 1);
    return {
      x: -ds.offset[0] + width / 2 / ds.scale,
      y: -ds.offset[1] + height / 2 / ds.scale,
    };
  }

  clearCanvas(): void {
    this.state = { sessionId: null, graph: null, dirty: false };
    this.nodeMap.clear();
    this.subgraphStack = [];
    this._pendingWidgetMutations.clear();
    this.lGraph.clear();
    this.onNavigationChange?.([]);
  }

  async loadSession(sessionId: string): Promise<void> {
    await this.graphOps.loadSession(sessionId);
  }

  rebuildCanvas(def: NodeGraphDefinition): void {
    this.graphOps.rebuildCanvas(def);
  }

  async syncPositions(): Promise<void> {
    await this.graphOps.syncPositions();
  }

  resize(width: number, height: number): void {
    const dpr = window.devicePixelRatio || 1;
    (this.lCanvas as any).resize(Math.round(width * dpr), Math.round(height * dpr));
  }

  startPositionSync(intervalMs = 2000): () => void {
    const id = setInterval(() => this.syncPositions(), intervalMs);
    return () => clearInterval(id);
  }

  async reloadCurrentSession(): Promise<void> {
    await this.graphOps.reloadCurrentSession();
  }

  async syncInlineWidgetValues(): Promise<void> {
    await this.graphOps.syncInlineWidgetValues();
  }

  async flushPendingWidgetMutations(): Promise<void> {
    await this.graphOps.flushPendingWidgetMutations();
  }

  canUndo(): boolean {
    return this.graphOps.canUndo();
  }

  canRedo(): boolean {
    return this.graphOps.canRedo();
  }

  async undo(): Promise<void> {
    await this.graphOps.undo();
  }

  async redo(): Promise<void> {
    await this.graphOps.redo();
  }

  async toggleSelectedNodesDisabled(): Promise<void> {
    await this.interactions.toggleSelectedNodesDisabled();
  }

  async enterSubgraph(
    parentNodeDef: NodeDefinition,
    mode: "function" | "brain-tool",
    toolIndex?: number,
    toolDef?: BrainToolDefinition,
    functionConfig?: EmbeddedFunctionConfig,
  ): Promise<void> {
    await this.subgraphs.enterSubgraph(parentNodeDef, mode, toolIndex, toolDef, functionConfig);
  }

  async exitSubgraph(): Promise<void> {
    await this.subgraphs.exitSubgraph();
  }

  async exitSubgraphToDepth(targetDepth: number): Promise<void> {
    await this.subgraphs.exitSubgraphToDepth(targetDepth);
  }

  async flushSubgraphToRoot(): Promise<void> {
    await this.subgraphs.flushSubgraphToRoot();
  }

  async loadExternalSession(sessionId: string): Promise<void> {
    await this.subgraphs.loadExternalSession(sessionId);
  }

  async handleConnectionDropOnEmpty(
    sourceNodeId: string,
    sourcePortName: string,
    sourceType: string,
    isFromOutput: boolean,
    graphX: number,
    graphY: number,
  ): Promise<void> {
    await this.interactions.handleConnectionDropOnEmpty(
      sourceNodeId,
      sourcePortName,
      sourceType,
      isFromOutput,
      graphX,
      graphY,
    );
  }

  showCanvasContextMenu(event: MouseEvent, graphX: number, graphY: number): void {
    this.interactions.showCanvasContextMenu(event, graphX, graphY);
  }

  showPortBindingMenu(lNode: any, slotIndex: number, portName: string, event: MouseEvent): void {
    this.interactions.showPortBindingMenu(lNode, slotIndex, portName, event);
  }

  showNodeHelpDialog(lNode: any): void {
    this.interactions.showNodeHelpDialog(lNode);
  }
}
