import "./ui/theme.css";
import { initTheme, loadThemes, getThemeNames, getCurrentThemeName, setTheme } from "./ui/theme";
import { registry, graphs } from "./api/client";
import { ws } from "./api/ws";
import type { NodeTypeInfo } from "./api/types";
import { registerNodeTypes } from "./graph/registry";
import { ZihuanCanvas } from "./graph/canvas";
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
import { TabManager } from "./app/tab_manager";
import { SaveManager } from "./app/save_manager";
import { GraphActions } from "./app/graph_actions";
import { WorkspaceController } from "./app/workspace_controller";

async function main() {
  initTheme();
  await loadThemes();
  injectStyles();
  const { toolbar, canvasContainer, canvasEl, backArrow } = buildDOM();

  ws.connect();

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
  let runningSessionId: string | null = null;
  let updateRunButton: (isRunning: boolean) => void = () => {};
  let appendLogEntry: (level: string, message: string, timestamp: string) => void = () => {};

  const tabs = new TabManager({
    onSwitchTab: (id) => { switchTab(id).catch(console.error); },
    onCloseTab: (id) => { closeTab(id).catch(console.error); },
    onCreateTab: () => { createNewTab().catch(console.error); },
  });

  const saveManager = new SaveManager({ canvas, tabs });
  const graphActions = new GraphActions({
    canvas,
    tabs,
    getNodeTypes: () => nodeTypes,
    getCurrentTaskId: () => currentTaskId,
  });
  const workspace = new WorkspaceController({
    canvas,
    tabs,
    getRunningSessionId: () => runningSessionId,
    updateRunButton: (isRunning) => updateRunButton(isRunning),
    createNewTab,
  });

  async function switchTab(id: string): Promise<void> {
    if (tabs.getActiveTabId() === id) return;
    tabs.setActiveTabId(id);
    await canvas.loadExternalSession(id);
    updateRunButton(id === runningSessionId);
    await workspace.persistWorkspaceState();
  }

  async function closeTab(id: string): Promise<void> {
    const activeBeforeClose = tabs.getActiveTabId();
    const { removed, index } = tabs.removeTab(id);
    if (!removed) return;

    try {
      await graphs.delete(id);
    } catch {}

    if (id === runningSessionId) {
      runningSessionId = null;
      updateRunButton(false);
    }

    if (activeBeforeClose === id) {
      const next = tabs.getNextTabAfterRemoval(index);
      if (next) {
        await switchTab(next.id);
        return;
      }
      canvas.clearCanvas();
      updateRunButton(false);
      await workspace.persistWorkspaceState();
      return;
    }

    await workspace.persistWorkspaceState();
  }

  async function createNewTab(): Promise<void> {
    const tab = await graphs.create();
    await tabs.openTab(tab.id, "未命名", false);
    await canvas.loadExternalSession(tab.id);
    updateRunButton(tab.id === runningSessionId);
    await workspace.persistWorkspaceState();
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
    getActiveTabId: () => tabs.getActiveTabId(),
    setCurrentTaskId: (taskId) => { currentTaskId = taskId; },
    setRunningSessionId: (sessionId) => { runningSessionId = sessionId; },
    updateRunButton: (isRunning) => updateRunButton(isRunning),
    addLog,
    appendLogEntry: (level, message, timestamp) => appendLogEntry(level, message, timestamp),
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
  const onUndo = () => { canvas.undo().catch(console.error); };
  const onRedo = () => { canvas.redo().catch(console.error); };

  const { updateUndoRedoButtons } = buildToolbar(
    toolbar,
    onNewGraph,
    onOpenFile,
    onSaveFile,
    onSaveAs,
    onSaveToWorkflows,
    onValidate,
    onBrowseWorkflows,
    () => { graphActions.openGraphMetadata().catch(console.error); },
    (msg) => showErrorDialog(msg),
    onUndo,
    onRedo,
    getThemeNames,
    getCurrentThemeName,
    setTheme,
  );

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
  } catch {
    // Startup restoration failed — user can create one manually.
  }
}

main().catch((e) => {
  console.error("Fatal startup error:", e);
  alert("应用启动失败，请查看控制台。");
});
