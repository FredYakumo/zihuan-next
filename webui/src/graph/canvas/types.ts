import type { LGraph, LGraphCanvas } from "litegraph.js";
import type { NodeGraphDefinition, NodeDefinition, NodeTypeInfo } from "../../api/types";
import { HistoryManager } from "../history";

export interface CanvasState {
  sessionId: string | null;
  graph: NodeGraphDefinition | null;
  dirty: boolean;
}

export interface SubgraphStackEntry {
  label: string;
  parentSessionId: string;
  virtualSessionId: string;
  saveBack: (modifiedGraph: NodeGraphDefinition) => Promise<void>;
}

export interface CanvasFacade {
  lGraph: InstanceType<typeof LGraph>;
  lCanvas: InstanceType<typeof LGraphCanvas>;
  state: CanvasState;
  nodeMap: Map<string, any>;
  subgraphStack: SubgraphStackEntry[];
  nodeClipboard: NodeDefinition[];
  _rebuilding: boolean;
  history: HistoryManager<NodeGraphDefinition>;
  _widgetMutationTimer: ReturnType<typeof setTimeout> | null;
  _nodeMoveTimer: ReturnType<typeof setTimeout> | null;
  _graphMutationTimer: ReturnType<typeof setTimeout> | null;
  _pendingWidgetMutations: Set<Promise<unknown>>;
  _pendingGraphMutations: Set<Promise<unknown>>;
  outputSummariesVisible: boolean;
  nodeTypes: NodeTypeInfo[];
  uiRenderer?: { mount(node: any, definition: NodeDefinition, info: NodeTypeInfo | undefined): void; reposition(): void; clear(): void; dispose(): void };
  onNavigationChange?: (labels: string[]) => void;
  onGraphDirty?: () => void;
  onHistoryChange?: () => void;
  onAddNodeRequest?: (graphX: number, graphY: number) => void;
  sessionId: string | null;
  rootSessionId: string | null;
  isInSubgraph: boolean;
  loadSession(sessionId: string): Promise<void>;
  reloadCurrentSession(): Promise<void>;
  syncInlineWidgetValues(): Promise<void>;
  flushPendingWidgetMutations(): Promise<void>;
  flushPendingGraphMutations(): Promise<void>;
  rebuildCanvas(def: NodeGraphDefinition): void;
  addLGraphNodeDirect(nodeDef: NodeDefinition): void;
  enterSubgraph(
    parentNodeDef: NodeDefinition,
    mode: "function" | "tool-calling-tool",
    toolIndex?: number,
    toolDef?: unknown,
    functionConfig?: unknown,
  ): Promise<void>;
  showNodeHelpDialog(lNode: any): void;
  showPortBindingMenu(lNode: any, slotIndex: number, portName: string, event: MouseEvent): void;
  showCanvasContextMenu(event: MouseEvent, graphX: number, graphY: number): void;
  handleConnectionDropOnEmpty(
    sourceNodeId: string,
    sourcePortName: string,
    sourceType: string,
    isFromOutput: boolean,
    graphX: number,
    graphY: number,
  ): Promise<void>;
  toggleOutputSummaries(): boolean;
}
