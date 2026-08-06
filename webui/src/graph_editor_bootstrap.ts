import "./ui/theme.css";
import { initTheme, loadThemes, getThemeNames, getCurrentThemeName, setTheme } from "./ui/theme";
import { registry, graphs, workflows, fileIO, tasks } from "./api/client";
import { ws } from "./api/ws";
import type { NodeTypeInfo, TaskEntry } from "./api/types";
import { registerNodeTypes } from "./graph/registry";
import { ZihuanCanvas } from "./graph/canvas";
import { installPreviewWsHandler } from "./graph/node_widgets/qq_message_preview";
import {
  buildCanvasPanelButtons,
  buildDOM,
  buildToolbar,
  createLogToastOverlay,
  injectStyles,
  updateBreadcrumb,
} from "./ui/shell/index";
import { showErrorDialog } from "./ui/dialogs/index";
import { registerGlobalShortcuts } from "./app/shortcuts";
import { observeCanvasResize, registerUnsavedChangesWarning, startAutoSaveLoop } from "./app/lifecycle";
import { registerTaskRuntimeHandlers } from "./app/task_runtime";
import { TaskManagerStore } from "./app/task_manager";
import { TabManager } from "./app/tab_manager";
import { SaveManager } from "./app/save_manager";
import { GraphActions } from "./app/graph_actions";
import { WorkspaceController } from "./app/workspace_controller";
import { openTaskManagerDialog } from "./ui/dialogs/index";

const TASK_TRACE_NODE_PREFIX = "qq_chat_";
// Initial x-coordinate for the task trace graph layout.
const TASK_TRACE_ORIGIN_X = 60;
// Initial y-coordinate for the task trace graph layout.
const TASK_TRACE_ORIGIN_Y = 60;
// Maximum width of a task trace graph row before wrapping to the next row.
const TASK_TRACE_MAX_ROW_WIDTH = 1440;
// Horizontal gap between adjacent task trace nodes in the same row.
const TASK_TRACE_HORIZONTAL_GAP = 72;
// Vertical gap between adjacent task trace graph rows.
const TASK_TRACE_VERTICAL_GAP = 80;

export async function bootstrapGraphEditor() {
  initTheme();
  await loadThemes();
  injectStyles();
  const { toolbar, canvasContainer, canvasEl, backArrow } = buildDOM();

  ws.connect();
  installPreviewWsHandler(ws);

  let nodeTypes: NodeTypeInfo[] = [];
  try {
    const reg = await registry.getTypes();
    nodeTypes = reg.types;
    registerNodeTypes(nodeTypes);
  } catch (e) {
    console.error("Failed to load registry:", e);
    showErrorDialog("加载节点注册表失败: " + (e as Error).message);
  }

  const canvas = new ZihuanCanvas(canvasEl);
  canvas.nodeTypes = nodeTypes;

  let currentTaskId: string | null = null;
  let updateRunButton: (isRunning: boolean) => void = () => {};
  let appendLogEntry: (level: string, message: string, timestamp: string) => void = () => {};
  let setToolbarTaskState: (tasks: TaskEntry[]) => void = () => {};
  const taskStore = new TaskManagerStore();

  const tabs = new TabManager({
    onSwitchTab: (id) => { switchTab(id).catch(console.error); },
    onCloseTab: (id) => { closeTab(id).catch(console.error); },
    onCreateTab: () => { createNewTab().catch(console.error); },
  });

  const saveManager = new SaveManager({
    canvas,
    tabs,
  });
  const graphActions = new GraphActions({
    canvas,
    tabs,
    getNodeTypes: () => nodeTypes,
    getCurrentTaskId: () => currentTaskId,
  });
  const workspace = new WorkspaceController({
    createNewTab,
  });

  async function switchTab(id: string): Promise<void> {
    if (tabs.getActiveTabId() === id) return;
    tabs.setActiveTabId(id);
    await canvas.loadExternalSession(id);
    updateRunButton(taskStore.getRunningTaskForSession(id) !== null);
  }

  async function closeTab(id: string): Promise<void> {
    const tab = tabs.findTab(id);
    if (!tab) return;
    if (tab.dirty) {
      const displayName = tab.name + (tab.isWorkflowSet ? " [工作流集]" : "");
      const confirmed = window.confirm(`“${displayName}”有未保存更改，确认关闭并放弃更改吗？`);
      if (!confirmed) return;
    }

    const activeBeforeClose = tabs.getActiveTabId();
    const { removed, index } = tabs.removeTab(id);
    if (!removed) return;

    try {
      await graphs.delete(id);
    } catch {}

    if (activeBeforeClose === id) {
      const next = tabs.getNextTabAfterRemoval(index);
      if (next) {
        await switchTab(next.id);
        return;
      }
      canvas.clearCanvas();
      updateRunButton(false);
      return;
    }
  }

  async function createNewTab(): Promise<void> {
    const tab = await graphs.create();
    await tabs.openTab(tab.id, "未命名", false);
    await canvas.loadExternalSession(tab.id);
    updateRunButton(taskStore.getRunningTaskForSession(tab.id) !== null);
  }

  async function openWorkflowFromRoute(): Promise<void> {
    const params = new URLSearchParams(window.location.search);
    const workflowName = params.get("workflow");
    if (!workflowName) {
      return;
    }

    const detailed = await workflows.listDetailed();
    const target = detailed.workflows.find((item) => item.name === workflowName);
    if (!target) {
      throw new Error(`未找到工作流 ${workflowName}`);
    }

    const workflowPath = `workflow_set/${target.file}`;
    const existing = tabs.findWorkflowTabByPath(workflowPath);
    if (existing) {
      tabs.setActiveTabId(existing.id);
      await canvas.loadExternalSession(existing.id);
      updateRunButton(taskStore.getRunningTaskForSession(existing.id) !== null);
      return;
    }

    const loaded = await fileIO.open(workflowPath);
    await tabs.openTab(loaded.session_id, target.display_name ?? workflowName, false, true);
    tabs.updateTab(loaded.session_id, { workflowPath });
    await canvas.loadExternalSession(loaded.session_id);
    updateRunButton(taskStore.getRunningTaskForSession(loaded.session_id) !== null);
  }

  async function openTaskGraphFromRoute(): Promise<void> {
    const taskId = new URLSearchParams(window.location.search).get("task_graph");
    if (!taskId) return;

    const snapshot = await tasks.graph(taskId);
    const tab = await graphs.create();
    await graphs.put(tab.id, snapshot);
    await tabs.openTab(tab.id, snapshot.metadata.name ?? `任务节点图 ${taskId.slice(0, 8)}`, false);
    await canvas.loadExternalSession(tab.id);
    await layoutTaskTraceGraph(canvas);
    // This session is a fresh copy of the immutable task snapshot. Saving it can
    // never overwrite the task record; users can use Save As to make a workflow.
    tabs.setTabDirty(tab.id, false);
    updateRunButton(false);
  }

  backArrow.querySelector("button")!.addEventListener("click", () => {
    canvas.exitSubgraph().catch((e: Error) => {
      showErrorDialog(`退出子图失败: ${e.message}`);
    });
  });

  canvas.onNavigationChange = (labels) => {
    updateBreadcrumb(labels, (depth) => {
      canvas.exitSubgraphToDepth(depth).catch((e: Error) => {
        showErrorDialog(`导航失败: ${e.message}`);
      });
    });
    backArrow.style.display = labels.length > 0 ? "" : "none";
  };

  canvas.onGraphDirty = () => {
    const sid = canvas.rootSessionId;
    if (sid) tabs.setTabDirty(sid, true);
  };

  const addLog = createLogToastOverlay(canvasContainer);
  registerTaskRuntimeHandlers(ws, {
    onTaskLifecycleChanged: () => {
      taskStore.refresh().catch(console.error);
    },
    addLog,
    appendLogEntry: (level, message, timestamp) => appendLogEntry(level, message, timestamp),
  });

  ws.onMessage((msg) => {
    if (msg.type === "TaskFinished" && !msg.success) {
      const summary = msg.error?.trim() || "执行失败";
      showErrorDialog(
        `执行失败: ${summary}\n\n任务 ID: ${msg.task_id}\n详细信息请到任务管理器的“日志”中查看。`
      );
    }
  });

  canvas.onAddNodeRequest = (gx, gy) => {
    graphActions.addNodeWithDialog(gx, gy).catch(console.error);
  };

  ({ updateRunButton, appendLogEntry } = buildCanvasPanelButtons(
    canvasContainer,
    () => { graphActions.openHyperparameters().catch(console.error); },
    () => { graphActions.openVariables().catch(console.error); },
    () => { graphActions.addNodeWithDialog().catch(console.error); },
    () => { graphActions.execute().catch(console.error); },
    () => { graphActions.stopTask().catch(console.error); },
    () => canvas.toggleOutputSummaries(),
  ));

  const onNewGraph = () => createNewTab().catch((e: Error) => {
    showErrorDialog(`新建节点图失败: ${e.message}`);
  });
  const onOpenFile = () => graphActions.openLocalFile().catch(console.error);
  const onSaveFile = () => saveManager.saveCurrent().catch(console.error);
  const onSaveAs = () => saveManager.saveCurrentAs().catch(console.error);
  const onSaveToWorkflows = () => saveManager.saveCurrentToWorkflows().catch(console.error);
  const onValidate = () => graphActions.validate().catch(console.error);
  const onBrowseWorkflows = () => graphActions.browseWorkflows().catch(console.error);
  const onBackToConsole = () => {
    window.location.href = "/";
  };
  const onUndo = () => { canvas.undo().catch(console.error); };
  const onRedo = () => { canvas.redo().catch(console.error); };

  const { updateUndoRedoButtons, setTaskState } = buildToolbar(
    toolbar,
    onNewGraph,
    onOpenFile,
    onSaveFile,
    onSaveAs,
    onSaveToWorkflows,
    onValidate,
    onBrowseWorkflows,
    () => { graphActions.openGraphMetadata().catch(console.error); },
    () => { graphActions.openGraphIO().catch(console.error); },
    () => openTaskManagerDialog(taskStore),
    (taskId) => { taskStore.stopTask(taskId).catch((error) => showErrorDialog(`结束任务失败: ${(error as Error).message}`)); },
    onBackToConsole,
    onUndo,
    onRedo,
    getThemeNames,
    getCurrentThemeName,
    setTheme,
  );
  setToolbarTaskState = setTaskState;

  taskStore.subscribe((tasks) => {
    setToolbarTaskState(tasks);
    currentTaskId = taskStore.getRunningTaskForSession(tabs.getActiveTabId())?.id ?? null;
    updateRunButton(currentTaskId !== null);
  });
  taskStore.start();

  canvas.onHistoryChange = () => {
    updateUndoRedoButtons(canvas.canUndo(), canvas.canRedo());
  };

  registerGlobalShortcuts({
    onNewGraph,
    onOpenFile,
    onSaveFile,
    onSaveAs,
    onUndo,
    onRedo,
    onToggleDisabled: () => canvas.toggleSelectedNodesDisabled(),
  });

  startAutoSaveLoop(() => saveManager.autoSaveDirtyTabs(), 30_000);
  registerUnsavedChangesWarning(() => tabs.hasDirtyTabs());

  const resizeCanvas = () => {
    const width = canvasContainer.clientWidth;
    const height = canvasContainer.clientHeight;
    if (width > 0 && height > 0) canvas.resize(width, height);
  };
  observeCanvasResize(canvasContainer, resizeCanvas);
  canvas.startPositionSync(3000);

  try {
    await workspace.restoreOrCreateTabs();
    await openWorkflowFromRoute();
    await openTaskGraphFromRoute();
  } catch {
    // Startup restoration failed — user can create one manually.
  }
}

async function layoutTaskTraceGraph(canvas: ZihuanCanvas): Promise<void> {
  const graph = canvas.state.graph;
  const sessionId = canvas.sessionId;
  if (!graph || !sessionId || graph.nodes.length === 0) return;
  if (!graph.nodes.every((node) => node.node_type.startsWith(TASK_TRACE_NODE_PREFIX))) return;

  const rows: Array<Array<{ id: string; width: number; height: number }>> = [];
  let rowWidth = 0;
  for (const nodeDef of graph.nodes) {
    const renderedNode = canvas.nodeMap.get(nodeDef.id);
    const width = Math.max(
      220,
      Number(renderedNode?.size?.[0] ?? nodeDef.size?.width ?? 220),
    );
    const height = Math.max(
      120,
      Number(renderedNode?.size?.[1] ?? nodeDef.size?.height ?? 120),
    );
    const nextWidth = rowWidth === 0 ? width : rowWidth + TASK_TRACE_HORIZONTAL_GAP + width;
    if (rows.length > 0 && rowWidth > 0 && nextWidth > TASK_TRACE_MAX_ROW_WIDTH) {
      rows.push([]);
      rowWidth = 0;
    }
    if (rows.length === 0) rows.push([]);
    const row = rows[rows.length - 1];
    rowWidth = row.length === 0 ? width : rowWidth + TASK_TRACE_HORIZONTAL_GAP + width;
    row.push({ id: nodeDef.id, width, height });
  }

  const positions = new Map<string, { x: number; y: number }>();
  let y = TASK_TRACE_ORIGIN_Y;
  for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
    const row = rows[rowIndex];
    const visualOrder = rowIndex % 2 === 0 ? row : [...row].reverse();
    let x = TASK_TRACE_ORIGIN_X;
    for (const node of visualOrder) {
      positions.set(node.id, { x, y });
      x += node.width + TASK_TRACE_HORIZONTAL_GAP;
    }
    y += Math.max(...row.map((node) => node.height)) + TASK_TRACE_VERTICAL_GAP;
  }

  const positionedGraph = {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const position = positions.get(node.id);
      return position ? { ...node, position } : node;
    }),
  };
  for (const [nodeId, position] of positions) {
    const renderedNode = canvas.nodeMap.get(nodeId);
    if (renderedNode) renderedNode.pos = [position.x, position.y];
  }
  canvas.state.graph = positionedGraph;
  canvas.lGraph.setDirtyCanvas(true, true);
  await graphs.put(sessionId, positionedGraph);
}
