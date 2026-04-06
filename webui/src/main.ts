// Zihuan Next — Web UI entry point

import { registry, graphs, fileIO, tasks, workflows as workflowsApi } from "./api/client";
import { ws } from "./api/ws";
import { registerNodeTypes } from "./graph/registry";
import { ZihuanCanvas } from "./graph/canvas";
import { injectStyles, buildDOM, buildToolbar, buildCanvasPanelButtons, updateBreadcrumb, updateTabs } from "./ui/shell";
import type { TabInfo } from "./ui/shell";
import { showWorkflowsDialog, openHyperparametersDialog, openVariablesDialog, showAddNodeDialog } from "./ui/dialogs";
import type { NodeTypeInfo } from "./api/types";

async function main() {
  injectStyles();
  const { toolbar, tabsBar: _tabsBar, sidebar: _sidebar, canvasContainer, canvasEl, backArrow, statusBar } = buildDOM();

  // Connect WebSocket
  ws.connect();

  // Load registry
  let nodeTypes: NodeTypeInfo[] = [];
  try {
    const reg = await registry.getTypes();
    nodeTypes = reg.types;
    registerNodeTypes(nodeTypes);
  } catch (e) {
    console.error("Failed to load registry:", e);
    statusBar.textContent = "Error: failed to load node registry";
  }

  // Create canvas
  const canvas = new ZihuanCanvas(canvasEl);

  // ── Tab state ────────────────────────────────────────────────────────────
  let tabList: TabInfo[] = [];
  let activeTabId: string | null = null;

  function renderTabs() {
    updateTabs(
      tabList,
      activeTabId,
      (id) => { switchTab(id).catch(console.error); },
      (id) => { closeTab(id).catch(console.error); },
      () => { createNewTab().catch(console.error); },
    );
    const activeTab = tabList.find((t) => t.id === activeTabId);
    document.title = activeTab
      ? `${activeTab.name} — Zihuan Next`
      : "Zihuan Next — Node Graph Editor";
  }

  async function switchTab(id: string) {
    if (activeTabId === id) return;
    activeTabId = id;
    renderTabs();
    await canvas.loadSession(id);
    updateRunButton(id === runningSessionId);
    const tab = tabList.find((t) => t.id === id);
    statusBar.textContent = tab ? `切换到: ${tab.name}` : "Ready";
  }

  async function closeTab(id: string) {
    const idx = tabList.findIndex((t) => t.id === id);
    if (idx === -1) return;
    tabList.splice(idx, 1);
    try { await graphs.delete(id); } catch { /* ignore */ }
    if (id === runningSessionId) {
      runningSessionId = null;
      updateRunButton(false);
    }
    if (activeTabId === id) {
      const next = tabList[Math.min(idx, tabList.length - 1)];
      if (next) {
        await switchTab(next.id);
      } else {
        activeTabId = null;
        canvas.clearCanvas();
        renderTabs();
        updateRunButton(false);
        statusBar.textContent = "所有标签已关闭 — 新建或打开节点图";
      }
    } else {
      renderTabs();
    }
  }

  function openTab(id: string, name: string, dirty = false) {
    const existing = tabList.findIndex((t) => t.id === id);
    if (existing !== -1) {
      tabList[existing].name = name;
      tabList[existing].dirty = dirty;
    } else {
      tabList.push({ id, name, dirty });
    }
    activeTabId = id;
    renderTabs();
  }

  function setTabDirty(id: string, dirty: boolean) {
    const tab = tabList.find((t) => t.id === id);
    if (tab) { tab.dirty = dirty; renderTabs(); }
  }

  /** Derive a display name from a file path or fall back to "未命名". */
  function tabNameFrom(filePath: string | null, fallback = "未命名"): string {
    if (!filePath) return fallback;
    const base = filePath.split(/[\\/]/).pop() ?? fallback;
    return base.replace(/\.json$/i, "");
  }

  async function createNewTab() {
    const tab = await graphs.create();
    openTab(tab.id, "未命名", false);
    await canvas.loadSession(tab.id);
    statusBar.textContent = "新建节点图";
  }

  // Wire breadcrumb navigation
  // Wire back-arrow button (exits one subgraph level)
  backArrow.querySelector("button")!.addEventListener("click", () => {
    canvas.exitSubgraph().catch((e: Error) => {
      statusBar.textContent = `Exit subgraph error: ${e.message}`;
    });
  });

  canvas.onNavigationChange = (labels) => {
    updateBreadcrumb(labels, (depth) => {
      canvas.exitSubgraphToDepth(depth).catch((e: Error) => {
        statusBar.textContent = `Navigation error: ${e.message}`;
      });
    });
    backArrow.style.display = labels.length > 0 ? "" : "none";
    if (labels.length > 0) {
      statusBar.textContent = `子图: ${labels[labels.length - 1]}`;
    }
  };

  // State: current running task id and which session started it
  let currentTaskId: string | null = null;
  let runningSessionId: string | null = null;
  // Late-bound so the WS handler can be registered before updateRunButton is available
  let updateRunButton: (isRunning: boolean) => void = () => {};
  ws.onMessage((msg) => {
    if (msg.type === "TaskStarted") {
      currentTaskId = msg.task_id;
      runningSessionId = msg.graph_session_id;
      if (msg.graph_session_id === activeTabId) {
        updateRunButton(true);
      }
    }
    if (msg.type === "TaskFinished" || msg.type === "TaskStopped") {
      currentTaskId = null;
      runningSessionId = null;
      updateRunButton(false);
    }
  });

  // ── Toolbar actions ──────────────────────────────────────────────────────

  const onNewGraph = async () => {
    try {
      await createNewTab();
    } catch (e) {
      statusBar.textContent = `Error: ${(e as Error).message}`;
    }
  };

  // Open from local file (replaces old Upload)
  const onOpenFile = async () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const result = await fileIO.upload(file);
        const name = file.name.replace(/\.json$/i, "");
        openTab(result.session_id, name, false);
        await canvas.loadSession(result.session_id);
        statusBar.textContent = `已打开: ${file.name}`;
      } catch (e) {
        statusBar.textContent = `Error: ${(e as Error).message}`;
      }
    };
    input.click();
  };

  // Open workflow from server workflows/ directory
  const onWorkflows = async () => {
    try {
      const result = await workflowsApi.list();
      if (result.files.length === 0) {
        alert("workflows/ 目录中没有节点图文件。");
        return;
      }
      const selected = await showWorkflowsDialog(result.files);
      if (!selected) return;
      const openResult = await fileIO.open("workflows/" + selected);
      const name = tabNameFrom("workflows/" + selected);
      openTab(openResult.session_id, name, false);
      await canvas.loadSession(openResult.session_id);
      if (openResult.migrated) statusBar.textContent = `已打开 workflow: ${selected} (端口类型已迁移)`;
      else statusBar.textContent = `已打开 workflow: ${selected}`;
    } catch (e) {
      statusBar.textContent = `Error: ${(e as Error).message}`;
    }
  };

  const onSaveFile = async () => {
    const sid = canvas.sessionId;
    if (!sid) {
      statusBar.textContent = "No graph open";
      return;
    }
    try {
      const result = await graphs.saveFile(sid);
      setTabDirty(sid, false);
      statusBar.textContent = `已保存: ${result.path}`;
    } catch (e) {
      // No server path yet — download as file
      const url = graphs.downloadUrl(sid);
      const a = document.createElement("a");
      a.href = url;
      a.download = "graph.json";
      a.click();
      statusBar.textContent = "正在下载节点图 JSON...";
    }
  };

  // Save current graph into workflows/ directory
  const onSaveToWorkflows = async () => {
    const sid = canvas.sessionId;
    if (!sid) { statusBar.textContent = "No graph open"; return; }
    const currentTab = tabList.find((t) => t.id === sid);
    const defaultName = currentTab?.name ?? "untitled";
    const name = prompt("保存到 workflows/ 目录，文件名:", defaultName);
    if (!name) return;
    try {
      const result = await workflowsApi.save(sid, name);
      const displayName = tabNameFrom(result.path, name);
      const existing = tabList.find((t) => t.id === sid);
      if (existing) { existing.name = displayName; existing.dirty = false; }
      renderTabs();
      statusBar.textContent = `已保存到 workflows: ${result.path}`;
    } catch (e) {
      statusBar.textContent = `Error: ${(e as Error).message}`;
    }
  };

  const onValidate = async () => {
    const sid = canvas.sessionId;
    if (!sid) { statusBar.textContent = "No graph open"; return; }
    try {
      const result = await graphs.validate(sid);
      if (result.has_errors) {
        const msgs = result.issues.map((i) => `[${i.severity}] ${i.message}`).join("\n");
        alert(`Validation errors:\n\n${msgs}`);
        statusBar.textContent = `Validation failed (${result.issues.length} issues)`;
      } else {
        const warnings = result.issues.filter((i) => i.severity === "warning");
        statusBar.textContent = warnings.length
          ? `Valid (${warnings.length} warnings)`
          : "Graph is valid ✓";
      }
    } catch (e) {
      statusBar.textContent = `Validate error: ${(e as Error).message}`;
    }
  };

  const onExecute = async () => {
    const sid = canvas.sessionId;
    if (!sid) { statusBar.textContent = "No graph open"; return; }
    try {
      const result = await graphs.execute(sid);
      statusBar.textContent = `Execution started (task ${result.task_id.slice(0, 8)})`;
    } catch (e) {
      statusBar.textContent = `Execute error: ${(e as Error).message}`;
    }
  };

  const onStopTask = async () => {
    if (!currentTaskId) { statusBar.textContent = "No running task"; return; }
    try {
      await tasks.stop(currentTaskId);
      statusBar.textContent = "Stop requested";
    } catch (e) {
      statusBar.textContent = `Stop error: ${(e as Error).message}`;
    }
  };

  const onHyperparameters = () => {
    const sid = canvas.sessionId;
    if (!sid) { statusBar.textContent = "请先打开一个节点图"; return; }
    openHyperparametersDialog(sid, () => { canvas.reloadCurrentSession().catch(console.error); }).catch(console.error);
  };

  const onVariables = () => {
    const sid = canvas.sessionId;
    if (!sid) { statusBar.textContent = "请先打开一个节点图"; return; }
    openVariablesDialog(sid, () => { canvas.reloadCurrentSession().catch(console.error); }).catch(console.error);
  };

  const addNodeWithDialog = async (graphX?: number, graphY?: number) => {
    const sid = canvas.sessionId;
    if (!sid) { alert("请先打开一个节点图"); return; }
    const typeId = await showAddNodeDialog(nodeTypes);
    if (!typeId) return;
    const pos = graphX !== undefined && graphY !== undefined
      ? { x: graphX, y: graphY }
      : canvas.graphCenterPos();
    try {
      await graphs.addNode(sid, typeId, undefined, pos.x, pos.y);
      await canvas.loadSession(sid);
      setTabDirty(sid, true);
      statusBar.textContent = `已添加节点: ${typeId}`;
    } catch (e) {
      console.error("addNode error:", e);
      statusBar.textContent = `Error: ${(e as Error).message}`;
    }
  };

  canvas.onAddNodeRequest = (gx, gy) => { addNodeWithDialog(gx, gy).catch(console.error); };

  ({ updateRunButton } = buildCanvasPanelButtons(
    canvasContainer,
    onHyperparameters,
    onVariables,
    () => { addNodeWithDialog().catch(console.error); },
    onExecute,
    onStopTask,
  ));

  buildToolbar(
    toolbar,
    statusBar,
    onNewGraph,
    onOpenFile,
    onSaveFile,
    onSaveToWorkflows,
    onValidate,
  );

  // Auto-resize canvas to fill its container
  const resizeCanvas = () => {
    const w = canvasContainer.clientWidth;
    const h = canvasContainer.clientHeight;
    if (w > 0 && h > 0) canvas.resize(w, h);
  };
  resizeCanvas();
  new ResizeObserver(resizeCanvas).observe(canvasContainer);

  // Start position sync
  canvas.startPositionSync(3000);

  // ── Create default unnamed graph on startup ──────────────────────────────
  try {
    await createNewTab();
  } catch (e) {
    statusBar.textContent = "Ready — create or open a graph to begin";
  }
}

main().catch(console.error);

