// Zihuan Next — Web UI entry point

import "./ui/theme.css";
import { initTheme } from "./ui/theme";
import { registry, graphs, fileIO, tasks, workflows as workflowsApi } from "./api/client";
import { ws } from "./api/ws";
import { registerNodeTypes } from "./graph/registry";
import { ZihuanCanvas } from "./graph/canvas";
import { injectStyles, buildDOM, buildToolbar, buildCanvasPanelButtons, updateBreadcrumb, updateTabs, createLogToastOverlay } from "./ui/shell";
import type { TabInfo } from "./ui/shell";
import { showWorkflowsDialog, openHyperparametersDialog, openVariablesDialog, showAddNodeDialog, showSaveAsDialog, showWorkflowBrowserDialog, showErrorDialog } from "./ui/dialogs";
import type { NodeTypeInfo } from "./api/types";

async function main() {
  initTheme();
  injectStyles();
  const { toolbar, tabsBar: _tabsBar, sidebar: _sidebar, canvasContainer, canvasEl, backArrow } = buildDOM();

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
    showErrorDialog("加载节点注册表失败: " + (e as Error).message);
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
    if (activeTab) {
      const suffix = activeTab.isWorkflowSet ? " [工作流集]" : "";
      document.title = `${activeTab.name}${suffix} — Zihuan Next`;
    } else {
      document.title = "Zihuan Next — Node Graph Editor";
    }
  }

  async function switchTab(id: string) {
    if (activeTabId === id) return;
    activeTabId = id;
    renderTabs();
    await canvas.loadSession(id);
    updateRunButton(id === runningSessionId);
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
      }
    } else {
      renderTabs();
    }
  }

  function openTab(id: string, name: string, dirty = false, isWorkflowSet = false) {
    const existing = tabList.findIndex((t) => t.id === id);
    if (existing !== -1) {
      tabList[existing].name = name;
      tabList[existing].dirty = dirty;
      tabList[existing].isWorkflowSet = isWorkflowSet;
    } else {
      tabList.push({ id, name, dirty, isWorkflowSet });
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
  }

  // Wire breadcrumb navigation
  // Wire back-arrow button (exits one subgraph level)
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
    if (sid) setTabDirty(sid, true);
  };

  // Log overlay (top-left toast) — created here so the WS handler below can close over addLog
  const addLog = createLogToastOverlay(canvasContainer);

  // State: current running task id and which session started it
  let currentTaskId: string | null = null;
  let runningSessionId: string | null = null;
  // Late-bound so the WS handler can be registered before updateRunButton is available
  let updateRunButton: (isRunning: boolean) => void = () => {};
  let appendLogEntry: (level: string, message: string, timestamp: string) => void = () => {};
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
    if (msg.type === "LogMessage") {
      addLog(msg.level, msg.message);
      appendLogEntry(msg.level, msg.message, msg.timestamp);
    }
  });

  // ── Toolbar actions ──────────────────────────────────────────────────────

  const onNewGraph = async () => {
    try {
      await createNewTab();
    } catch (e) {
      showErrorDialog(`新建节点图失败: ${(e as Error).message}`);
    }
  };

  // Open from local file — uses File System Access API (Chrome/Edge) when available
  // so that Ctrl+S can write back to the same file without a prompt.
  const onOpenFile = async () => {
    // Prefer File System Access API (grants a reusable writable handle)
    if ("showOpenFilePicker" in window) {
      type ShowOpenFilePicker = (opts?: object) => Promise<FileSystemFileHandle[]>;
      try {
        const [handle] = await (window.showOpenFilePicker as ShowOpenFilePicker)({
          types: [{ description: "JSON", accept: { "application/json": [".json"] } }],
          multiple: false,
        });
        const file = await handle.getFile();
        const result = await fileIO.upload(file);
        const name = file.name.replace(/\.json$/i, "");
        openTab(result.session_id, name, false, false);
        const tab = tabList.find((t) => t.id === result.session_id);
        if (tab) tab.fileHandle = handle;
        await canvas.loadSession(result.session_id);
        return;
      } catch (e) {
        if ((e as Error).name === "AbortError") return; // user cancelled
        // Fall through to legacy input
      }
    }
    // Fallback: plain <input type="file"> (no write-back handle)
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const result = await fileIO.upload(file);
        const name = file.name.replace(/\.json$/i, "");
        openTab(result.session_id, name, false, false);
        await canvas.loadSession(result.session_id);
      } catch (e) {
        showErrorDialog(`打开文件失败: ${(e as Error).message}`);
      }
    };
    input.click();
  };

  // Open workflow from server workflow_set/ directory
  const onWorkflows = async () => {
    try {
      const result = await workflowsApi.list();
      if (result.files.length === 0) {
        alert("workflow_set/ 目录中没有节点图文件。");
        return;
      }
      const selected = await showWorkflowsDialog(result.files);
      if (!selected) return;
      const openResult = await fileIO.open("workflow_set/" + selected);
      const name = tabNameFrom("workflow_set/" + selected);
      openTab(openResult.session_id, name, false, true);
      await canvas.loadSession(openResult.session_id);
    } catch (e) {
      showErrorDialog(`打开 workflow 失败: ${(e as Error).message}`);
    }
  };

  /** Write the current session JSON through the given FileSystemFileHandle. */
  const writeViaFileHandle = async (sid: string, handle: FileSystemFileHandle): Promise<void> => {
    const resp = await fetch(graphs.downloadUrl(sid));
    const json = await resp.text();
    const writable = await handle.createWritable();
    await writable.write(json);
    await writable.close();
  };

  const onSaveFile = async () => {
    const sid = canvas.rootSessionId;
    if (!sid) {
      showErrorDialog("请先打开一个节点图");
      return;
    }
    const currentTab = tabList.find((t) => t.id === sid);
    // Save back to the original local file via File System Access API
    if (currentTab?.fileHandle) {
      try {
        await writeViaFileHandle(sid, currentTab.fileHandle);
        setTabDirty(sid, false);
      } catch (e) {
        showErrorDialog(`保存失败: ${(e as Error).message}`);
      }
      return;
    }
    if (currentTab?.isWorkflowSet) {
      // Smart save: opened from workflow set → save back silently
      try {
        const result = await workflowsApi.save(sid, currentTab.name);
        const displayName = tabNameFrom(result.path, currentTab.name);
        currentTab.name = displayName;
        currentTab.dirty = false;
        currentTab.isWorkflowSet = true;
        renderTabs();
      } catch (e) {
        showErrorDialog(`保存失败: ${(e as Error).message}`);
      }
      return;
    }
    try {
      await graphs.saveFile(sid);
      setTabDirty(sid, false);
    } catch {
      // No server path yet (new or uploaded file) — redirect to Save As
      await onSaveAs();
    }
  };

  const onSaveAs = async () => {
    const sid = canvas.rootSessionId;
    if (!sid) { showErrorDialog("请先打开一个节点图"); return; }
    const currentTab = tabList.find((t) => t.id === sid);
    const defaultName = currentTab?.name ?? "untitled";
    const choice = await showSaveAsDialog(defaultName);
    if (!choice) return;
    if (choice === "local") {
      const url = graphs.downloadUrl(sid);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${defaultName}.json`;
      a.click();
      if (currentTab) { currentTab.isWorkflowSet = false; renderTabs(); }
    } else {
      const name = prompt("保存到 workflow_set/ 目录，文件名:", defaultName);
      if (!name) return;
      try {
        const result = await workflowsApi.save(sid, name);
        const displayName = tabNameFrom(result.path, name);
        if (currentTab) {
          currentTab.name = displayName;
          currentTab.isWorkflowSet = true;
          currentTab.dirty = false;
        }
        renderTabs();
      } catch (e) {
        showErrorDialog(`保存失败: ${(e as Error).message}`);
      }
    }
  };

  // Save current graph into workflow_set/ directory
  const onSaveToWorkflows = async () => {
    const sid = canvas.rootSessionId;
    if (!sid) { showErrorDialog("请先打开一个节点图"); return; }
    const currentTab = tabList.find((t) => t.id === sid);
    const defaultName = currentTab?.name ?? "untitled";
    const name = prompt("保存到 workflow_set/ 目录，文件名:", defaultName);
    if (!name) return;
    try {
      const result = await workflowsApi.save(sid, name);
      const displayName = tabNameFrom(result.path, name);
      if (currentTab) {
        currentTab.name = displayName;
        currentTab.isWorkflowSet = true;
        currentTab.dirty = false;
      }
      renderTabs();
    } catch (e) {
      showErrorDialog(`保存失败: ${(e as Error).message}`);
    }
  };

  const onBrowseWorkflows = async () => {
    try {
      const result = await workflowsApi.listDetailed();
      const selected = await showWorkflowBrowserDialog(result.workflows);
      if (!selected) return;
      const openResult = await fileIO.open("workflow_set/" + selected);
      const name = tabNameFrom("workflow_set/" + selected);
      openTab(openResult.session_id, name, false, true);
      await canvas.loadSession(openResult.session_id);
    } catch (e) {
      showErrorDialog(`打开 workflow 失败: ${(e as Error).message}`);
    }
  };

  const onValidate = async () => {
    const sid = canvas.sessionId;
    if (!sid) { showErrorDialog("请先打开一个节点图"); return; }
    try {
      const result = await graphs.validate(sid);
      if (result.has_errors) {
        const msgs = result.issues.map((i) => `[${i.severity}] ${i.message}`).join("\n");
        showErrorDialog(`验证失败:\n\n${msgs}`);
      }
    } catch (e) {
      showErrorDialog(`验证失败: ${(e as Error).message}`);
    }
  };

  const onExecute = async () => {
    const sid = canvas.sessionId;
    if (!sid) { showErrorDialog("请先打开一个节点图"); return; }
    try {
      await graphs.execute(sid);
    } catch (e) {
      showErrorDialog(`执行失败: ${(e as Error).message}`);
    }
  };

  const onStopTask = async () => {
    if (!currentTaskId) { showErrorDialog("当前没有正在运行的任务"); return; }
    try {
      await tasks.stop(currentTaskId);
    } catch (e) {
      showErrorDialog(`停止任务失败: ${(e as Error).message}`);
    }
  };

  const onHyperparameters = () => {
    const sid = canvas.rootSessionId;
    if (!sid) { showErrorDialog("请先打开一个节点图"); return; }
    openHyperparametersDialog(sid, () => { canvas.reloadCurrentSession().catch(console.error); }).catch(console.error);
  };

  const onVariables = () => {
    const sid = canvas.sessionId;
    if (!sid) { showErrorDialog("请先打开一个节点图"); return; }
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
    } catch (e) {
      console.error("addNode error:", e);
      showErrorDialog(`添加节点失败: ${(e as Error).message}`);
    }
  };

  canvas.onAddNodeRequest = (gx, gy) => { addNodeWithDialog(gx, gy).catch(console.error); };

  ({ updateRunButton, appendLogEntry } = buildCanvasPanelButtons(
    canvasContainer,
    onHyperparameters,
    onVariables,
    () => { addNodeWithDialog().catch(console.error); },
    onExecute,
    onStopTask,
  ));

  buildToolbar(
    toolbar,
    onNewGraph,
    onOpenFile,
    onSaveFile,
    onSaveAs,
    onSaveToWorkflows,
    onValidate,
    onBrowseWorkflows,
    (msg) => showErrorDialog(msg),
  );

  // ── Global keyboard shortcuts ────────────────────────────────────────────
  document.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    const inInput = target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
    if (!e.ctrlKey && !e.metaKey) return;
    if (e.key === "n" && !e.shiftKey && !inInput) {
      e.preventDefault();
      onNewGraph();
    } else if (e.key === "o" && !e.shiftKey && !inInput) {
      e.preventDefault();
      onOpenFile();
    } else if (e.key === "s" && e.shiftKey) {
      e.preventDefault();
      onSaveAs();
    } else if (e.key === "s" && !e.shiftKey) {
      e.preventDefault();
      onSaveFile();
    }
  });

  // ── Auto-save ──────────────────────────────────────────────────────────────
  // Every 30 s: silently save dirty tabs that have a known server path.
  // New tabs and uploaded files without a server path are skipped.
  const autoSave = async () => {
    for (const tab of tabList) {
      if (!tab.dirty) continue;
      try {
        if (tab.fileHandle) {
          await writeViaFileHandle(tab.id, tab.fileHandle);
          tab.dirty = false;
          renderTabs();
        } else if (tab.isWorkflowSet) {
          const result = await workflowsApi.save(tab.id, tab.name);
          const displayName = tabNameFrom(result.path, tab.name);
          tab.name = displayName;
          tab.dirty = false;
          tab.isWorkflowSet = true;
          renderTabs();
        } else {
          // Try saving to the session's server path; silently skip if none.
          await graphs.saveFile(tab.id);
          setTabDirty(tab.id, false);
        }
      } catch {
        // No path available — skip (new or uploaded file)
      }
    }
  };
  setInterval(() => { autoSave().catch(console.error); }, 30_000);

  // Warn before leaving the page when there are unsaved dirty tabs
  window.addEventListener("beforeunload", (e) => {
    if (tabList.some((t) => t.dirty)) {
      e.preventDefault();
    }
  });

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
  } catch {
    // Startup graph creation failed — user can create one manually
  }
}

main().catch(console.error);

