// App shell — creates the toolbar, canvas, and sidebar DOM elements

import type { NodeTypeInfo } from "../api/types";
import { graphs, fileIO, tasks } from "../api/client";
import { ws } from "../api/ws";
import type { TaskEntry } from "../api/types";

const STYLES = `
  :root {
    --bg: #1a1a2e;
    --toolbar-bg: #16213e;
    --sidebar-bg: #0f3460;
    --text: #e0e0e0;
    --accent: #e94560;
    --border: #2a2a4a;
    --node-hover: #1a3a6e;
  }

  #toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--toolbar-bg);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    height: 44px;
    color: var(--text);
    font-family: sans-serif;
    font-size: 13px;
    position: relative;
  }

  #toolbar .title {
    font-weight: bold;
    color: var(--accent);
    margin-right: 8px;
    cursor: pointer;
    user-select: none;
    transition: opacity 0.15s;
  }

  #toolbar .title:hover {
    opacity: 0.8;
  }

  #toolbar-menu {
    position: absolute;
    top: 44px;
    left: 12px;
    background: var(--toolbar-bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    z-index: 1000;
    min-width: 180px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    display: none;
    flex-direction: column;
  }

  #toolbar-menu.open {
    display: flex;
  }

  #toolbar-menu .menu-item {
    padding: 8px 16px;
    cursor: pointer;
    color: var(--text);
    font-size: 13px;
    font-family: sans-serif;
    white-space: nowrap;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
  }

  #toolbar-menu .menu-item:hover {
    background: var(--node-hover);
  }

  #toolbar-menu .menu-item .menu-shortcut {
    color: #888;
    font-size: 11px;
    flex-shrink: 0;
  }

  #toolbar .spacer {
    flex: 1;
  }

  #toolbar .task-status {
    font-size: 12px;
    color: #aaa;
  }

  #toolbar .task-status.running {
    color: #4caf50;
  }

  #main {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  #sidebar {
    display: none;
  }

  #sidebar .category-header {
    padding: 6px 10px;
    font-weight: bold;
    color: #8ab4f8;
    background: rgba(0,0,0,0.2);
    border-bottom: 1px solid var(--border);
    user-select: none;
    cursor: pointer;
  }

  #sidebar .node-item {
    padding: 5px 16px;
    cursor: grab;
    border-bottom: 1px solid rgba(255,255,255,0.04);
    color: var(--text);
    transition: background 0.1s;
  }

  #sidebar .node-item:hover {
    background: var(--node-hover);
  }

  #sidebar .category-group.collapsed .node-item {
    display: none;
  }

  #canvas-container {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  #graph-canvas {
    position: absolute;
    inset: 0;
    display: block;
  }

  #graph-tabs {
    display: flex;
    gap: 4px;
    padding: 4px 8px 0;
    align-items: flex-end;
    background: #0d1117;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    min-height: 30px;
  }

  #graph-tabs .tab {
    padding: 4px 10px;
    border-radius: 3px 3px 0 0;
    border: 1px solid var(--border);
    border-bottom: none;
    background: #1a2a4a;
    cursor: pointer;
    font-size: 12px;
    color: #aaa;
  }

  #graph-tabs .tab.active {
    background: var(--toolbar-bg);
    color: var(--text);
  }

  #graph-tabs .new-tab {
    padding: 2px 8px;
    font-size: 18px;
    cursor: pointer;
    color: #aaa;
    border: none;
    background: transparent;
  }

  #graph-tabs .new-tab:hover {
    color: var(--text);
  }

  #graph-tabs .tab .tab-close {
    margin-left: 6px;
    font-size: 14px;
    line-height: 1;
    color: #666;
    border: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
    display: inline-flex;
    align-items: center;
  }

  #graph-tabs .tab .tab-close:hover {
    color: #e94560;
  }

  #graph-tabs .tab.active .tab-close {
    color: #aaa;
  }

  #status-bar {
    height: 24px;
    background: #0d1117;
    border-top: 1px solid var(--border);
    padding: 0 12px;
    display: flex;
    align-items: center;
    font-size: 11px;
    color: #666;
    font-family: monospace;
    flex-shrink: 0;
  }

  #breadcrumb {
    display: flex;
    align-items: center;
    padding: 0 12px;
    height: 26px;
    background: #0d1117;
    border-bottom: 1px solid #2a2a4a;
    font-size: 12px;
    color: #aaa;
    flex-shrink: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  #breadcrumb.hidden {
    display: none;
  }

  #breadcrumb .bc-root {
    color: #8ab4f8;
    cursor: pointer;
  }

  #breadcrumb .bc-root:hover {
    text-decoration: underline;
  }

  #breadcrumb .bc-sep {
    margin: 0 5px;
    color: #555;
  }

  #breadcrumb .bc-item {
    color: #ccc;
  }

  #breadcrumb .bc-item.bc-clickable {
    cursor: pointer;
  }

  #breadcrumb .bc-item.bc-clickable:hover {
    text-decoration: underline;
    color: #8ab4f8;
  }

  #canvas-back-arrow {
    position: absolute;
    top: 12px;
    left: 12px;
    z-index: 100;
    display: none;
  }

  #canvas-back-arrow button {
    padding: 6px 14px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: rgba(22, 33, 62, 0.92);
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
    font-family: sans-serif;
    backdrop-filter: blur(4px);
    transition: background 0.15s;
    white-space: nowrap;
  }

  #canvas-back-arrow button:hover {
    background: var(--node-hover);
  }

  #canvas-panel-buttons {
    position: absolute;
    right: 12px;
    bottom: 12px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  #canvas-panel-buttons button {
    padding: 6px 14px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: rgba(22, 33, 62, 0.92);
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
    font-family: sans-serif;
    backdrop-filter: blur(4px);
    transition: background 0.15s;
    white-space: nowrap;
  }

  #canvas-panel-buttons button:hover {
    background: var(--node-hover);
  }

  #canvas-panel-buttons #btn-run {
    border-color: #4caf50;
    color: #4caf50;
  }

  #canvas-panel-buttons #btn-run.stop {
    border-color: var(--accent);
    color: var(--accent);
  }
`;

export function injectStyles(): void {
  const style = document.createElement("style");
  style.textContent = STYLES;
  document.head.appendChild(style);
}

export function buildDOM(): {
  toolbar: HTMLElement;
  tabsBar: HTMLElement;
  breadcrumb: HTMLElement;
  sidebar: HTMLElement;
  canvasContainer: HTMLElement;
  canvasEl: HTMLCanvasElement;
  backArrow: HTMLElement;
  statusBar: HTMLElement;
} {
  const app = document.getElementById("app")!;
  app.style.flexDirection = "column";

  // Toolbar
  const toolbar = document.createElement("div");
  toolbar.id = "toolbar";
  toolbar.innerHTML = `<span class="title">Zihuan Next</span>`;

  // Tabs bar
  const tabsBar = document.createElement("div");
  tabsBar.id = "graph-tabs";

  // Breadcrumb bar (hidden until inside a subgraph)
  const breadcrumb = document.createElement("div");
  breadcrumb.id = "breadcrumb";
  breadcrumb.className = "hidden";

  // Main area
  const main = document.createElement("div");
  main.id = "main";

  // Sidebar
  const sidebar = document.createElement("div");
  sidebar.id = "sidebar";

  // Canvas container
  const canvasContainer = document.createElement("div");
  canvasContainer.id = "canvas-container";

  // Canvas
  const canvasEl = document.createElement("canvas");
  canvasEl.id = "graph-canvas";
  canvasContainer.appendChild(canvasEl);

  // Back arrow overlay (top-left of canvas, shown when inside a subgraph)
  const backArrow = document.createElement("div");
  backArrow.id = "canvas-back-arrow";
  const backArrowBtn = document.createElement("button");
  backArrowBtn.textContent = "← 返回";
  backArrow.appendChild(backArrowBtn);
  canvasContainer.appendChild(backArrow);

  main.appendChild(sidebar);
  main.appendChild(canvasContainer);

  // Status bar
  const statusBar = document.createElement("div");
  statusBar.id = "status-bar";
  statusBar.textContent = "Ready";

  app.appendChild(toolbar);
  app.appendChild(tabsBar);
  app.appendChild(breadcrumb);
  app.appendChild(main);
  app.appendChild(statusBar);

  return { toolbar, tabsBar, breadcrumb, sidebar, canvasContainer, canvasEl, backArrow, statusBar };
}

/** Update the breadcrumb navigation bar. Pass empty array to clear/hide.
 *  onNavigateTo(depth) is called when the user clicks a breadcrumb segment:
 *  depth=0 means root ("主图"), depth=N means the Nth subgraph level. */
export function updateBreadcrumb(
  labels: string[],
  onNavigateTo?: (depth: number) => void,
): void {
  const breadcrumb = document.getElementById("breadcrumb");
  if (!breadcrumb) return;

  if (labels.length === 0) {
    breadcrumb.className = "hidden";
    breadcrumb.innerHTML = "";
    return;
  }

  breadcrumb.className = "";
  breadcrumb.innerHTML = "";

  const root = document.createElement("span");
  root.className = "bc-root";
  root.textContent = "主图";
  if (onNavigateTo) {
    root.addEventListener("click", () => onNavigateTo(0));
  }
  breadcrumb.appendChild(root);

  for (let i = 0; i < labels.length; i++) {
    const sep = document.createElement("span");
    sep.className = "bc-sep";
    sep.textContent = "›";
    breadcrumb.appendChild(sep);

    const item = document.createElement("span");
    const isLast = i === labels.length - 1;
    item.className = isLast ? "bc-item" : "bc-item bc-clickable";
    item.textContent = labels[i];
    if (!isLast && onNavigateTo) {
      item.addEventListener("click", () => onNavigateTo(i + 1));
    }
    breadcrumb.appendChild(item);
  }
}

export interface TabInfo {
  id: string;
  name: string;
  dirty: boolean;
  isWorkflowSet: boolean;
}

/**
 * Re-render the graph tabs bar. Calls onSwitch(id) when a tab is clicked and
 * onClose(id) when the × button is pressed.
 */
export function updateTabs(
  tabs: TabInfo[],
  activeId: string | null,
  onSwitch: (id: string) => void,
  onClose: (id: string) => void,
  onNew: () => void,
): void {
  const bar = document.getElementById("graph-tabs");
  if (!bar) return;
  bar.innerHTML = "";

  for (const tab of tabs) {
    const el = document.createElement("div");
    el.className = "tab" + (tab.id === activeId ? " active" : "");
    el.title = tab.name;

    const label = document.createElement("span");
    const displayName = tab.name + (tab.isWorkflowSet ? " [工作流集]" : "");
    label.textContent = (tab.dirty ? "* " : "") + displayName;
    el.appendChild(label);

    const closeBtn = document.createElement("button");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      onClose(tab.id);
    });
    el.appendChild(closeBtn);

    el.addEventListener("click", () => onSwitch(tab.id));
    bar.appendChild(el);
  }

  const newBtn = document.createElement("button");
  newBtn.className = "new-tab";
  newBtn.textContent = "+";
  newBtn.title = "新建节点图";
  newBtn.addEventListener("click", onNew);
  bar.appendChild(newBtn);
}

export function buildSidebar(
  sidebar: HTMLElement,
  nodeTypes: NodeTypeInfo[],
  onDrop: (typeId: string, x: number, y: number) => void,
  canvasContainer: HTMLElement
): void {
  sidebar.innerHTML = "";

  // Group by category
  const categories = new Map<string, NodeTypeInfo[]>();
  for (const nt of nodeTypes) {
    // Internal boundary nodes should not appear in the palette
    if (nt.category === "内部") continue;
    if (!categories.has(nt.category)) categories.set(nt.category, []);
    categories.get(nt.category)!.push(nt);
  }

  for (const [category, types] of categories) {
    const group = document.createElement("div");
    group.className = "category-group";

    const header = document.createElement("div");
    header.className = "category-header";
    header.textContent = category;
    header.addEventListener("click", () => group.classList.toggle("collapsed"));

    group.appendChild(header);

    for (const nt of types) {
      const item = document.createElement("div");
      item.className = "node-item";
      item.title = nt.description;
      item.textContent = nt.display_name;
      item.draggable = true;

      item.addEventListener("dragstart", (e) => {
        e.dataTransfer?.setData("application/zihuan-node-type", nt.type_id);
      });

      group.appendChild(item);
    }

    sidebar.appendChild(group);
  }

  // Handle drop onto canvas
  canvasContainer.addEventListener("dragover", (e) => e.preventDefault());
  canvasContainer.addEventListener("drop", (e) => {
    e.preventDefault();
    const typeId = e.dataTransfer?.getData("application/zihuan-node-type");
    if (!typeId) return;
    const rect = canvasContainer.getBoundingClientRect();
    onDrop(typeId, e.clientX - rect.left, e.clientY - rect.top);
  });
}

export function buildCanvasPanelButtons(
  canvasContainer: HTMLElement,
  onHyperparameters: () => void,
  onVariables: () => void,
  onAddNode: () => void,
  onExecute: () => void,
  onStopTask: () => void,
): { updateRunButton: (isRunning: boolean) => void } {
  const panel = document.createElement("div");
  panel.id = "canvas-panel-buttons";

  const addNodeBtn = document.createElement("button");
  addNodeBtn.textContent = "＋ 新建节点";
  addNodeBtn.title = "添加节点到画布";
  addNodeBtn.addEventListener("click", onAddNode);
  panel.appendChild(addNodeBtn);

  const hpBtn = document.createElement("button");
  hpBtn.textContent = "⚙ 超参数";
  hpBtn.title = "管理超参数定义和值";
  hpBtn.addEventListener("click", onHyperparameters);
  panel.appendChild(hpBtn);

  const varBtn = document.createElement("button");
  varBtn.textContent = "{ } 变量";
  varBtn.title = "管理图变量";
  varBtn.addEventListener("click", onVariables);
  panel.appendChild(varBtn);

  const runBtn = document.createElement("button");
  runBtn.id = "btn-run";
  runBtn.textContent = "Run ▶";
  runBtn.title = "运行当前工作流";
  let currentHandler = onExecute;
  runBtn.addEventListener("click", () => currentHandler());
  panel.appendChild(runBtn);

  canvasContainer.appendChild(panel);

  function updateRunButton(isRunning: boolean): void {
    if (isRunning) {
      runBtn.textContent = "Stop ■";
      runBtn.title = "停止当前任务";
      runBtn.classList.add("stop");
      currentHandler = onStopTask;
    } else {
      runBtn.textContent = "Run ▶";
      runBtn.title = "运行当前工作流";
      runBtn.classList.remove("stop");
      currentHandler = onExecute;
    }
  }

  return { updateRunButton };
}

export function buildToolbar(
  toolbar: HTMLElement,
  statusBar: HTMLElement,
  onNewGraph: () => void,
  onOpenFile: () => void,
  onSaveFile: () => void,
  onSaveAs: () => void,
  onSaveToWorkflows: () => void,
  onValidate: () => void,
): void {
  // Make the title a popup trigger
  const titleEl = toolbar.querySelector<HTMLElement>(".title")!;

  const menu = document.createElement("div");
  menu.id = "toolbar-menu";

  const menuItems: Array<{ label: string; shortcut?: string; onClick: () => void }> = [
    { label: "新建", shortcut: "Ctrl+N", onClick: onNewGraph },
    { label: "打开...", shortcut: "Ctrl+O", onClick: onOpenFile },
    { label: "保存", shortcut: "Ctrl+S", onClick: onSaveFile },
    { label: "另存为", shortcut: "Ctrl+Shift+S", onClick: onSaveAs },
    { label: "保存为工作流集", onClick: onSaveToWorkflows },
    { label: "验证", onClick: onValidate },
  ];

  for (const item of menuItems) {
    const el = document.createElement("div");
    el.className = "menu-item";
    const labelSpan = document.createElement("span");
    labelSpan.textContent = item.label;
    el.appendChild(labelSpan);
    if (item.shortcut) {
      const shortcutSpan = document.createElement("span");
      shortcutSpan.className = "menu-shortcut";
      shortcutSpan.textContent = item.shortcut;
      el.appendChild(shortcutSpan);
    }
    el.addEventListener("click", () => {
      menu.classList.remove("open");
      item.onClick();
    });
    menu.appendChild(el);
  }

  toolbar.appendChild(menu);

  titleEl.addEventListener("click", (e) => {
    e.stopPropagation();
    menu.classList.toggle("open");
  });

  document.addEventListener("click", () => {
    menu.classList.remove("open");
  });

  const spacer = document.createElement("span");
  spacer.className = "spacer";
  toolbar.appendChild(spacer);

  const taskStatus = document.createElement("span");
  taskStatus.className = "task-status";
  taskStatus.id = "task-status";
  taskStatus.textContent = "Idle";
  toolbar.appendChild(taskStatus);

  // Listen to WS for task updates
  ws.onMessage((msg) => {
    if (msg.type === "TaskStarted") {
      taskStatus.textContent = `Running: ${msg.graph_name}`;
      taskStatus.className = "task-status running";
      statusBar.textContent = `Execution started (task ${msg.task_id.slice(0, 8)})`;
    } else if (msg.type === "TaskFinished") {
      const success = msg.success;
      taskStatus.textContent = success ? "Done ✓" : "Failed ✗";
      taskStatus.className = "task-status";
      statusBar.textContent = success ? "Execution completed" : "Execution failed";
      setTimeout(() => {
        taskStatus.textContent = "Idle";
        statusBar.textContent = "Ready";
      }, 5000);
    } else if (msg.type === "TaskStopped") {
      taskStatus.textContent = "Stopped";
      taskStatus.className = "task-status";
      setTimeout(() => {
        taskStatus.textContent = "Idle";
      }, 3000);
    } else if (msg.type === "LogMessage") {
      statusBar.textContent = `[${msg.level}] ${msg.message}`;
    }
  });
}
