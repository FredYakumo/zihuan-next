// App shell — creates the toolbar, canvas, and sidebar DOM elements

import type { NodeTypeInfo } from "../api/types";
import type { ZihuanCanvas } from "../graph/canvas";
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
  }

  #toolbar .title {
    font-weight: bold;
    color: var(--accent);
    margin-right: 8px;
  }

  #toolbar button {
    padding: 4px 12px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: #1e3a5f;
    color: var(--text);
    cursor: pointer;
    transition: background 0.15s;
  }

  #toolbar button:hover {
    background: var(--node-hover);
  }

  #toolbar button.danger {
    border-color: var(--accent);
    color: var(--accent);
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
    width: 220px;
    flex-shrink: 0;
    background: var(--sidebar-bg);
    overflow-y: auto;
    border-right: 1px solid var(--border);
    color: var(--text);
    font-family: sans-serif;
    font-size: 12px;
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
    width: 100%;
    height: 100%;
    display: block;
  }

  #graph-tabs {
    display: flex;
    gap: 4px;
    padding: 0 8px;
    align-items: center;
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
    cursor: default;
  }

  #breadcrumb .bc-sep {
    margin: 0 5px;
    color: #555;
  }

  #breadcrumb .bc-item {
    color: #ccc;
  }
`;

export function injectStyles(): void {
  const style = document.createElement("style");
  style.textContent = STYLES;
  document.head.appendChild(style);
}

export function buildDOM(): {
  toolbar: HTMLElement;
  breadcrumb: HTMLElement;
  sidebar: HTMLElement;
  canvasContainer: HTMLElement;
  canvasEl: HTMLCanvasElement;
  statusBar: HTMLElement;
} {
  const app = document.getElementById("app")!;
  app.style.flexDirection = "column";

  // Toolbar
  const toolbar = document.createElement("div");
  toolbar.id = "toolbar";
  toolbar.innerHTML = `<span class="title">Zihuan Next</span>`;

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

  main.appendChild(sidebar);
  main.appendChild(canvasContainer);

  // Status bar
  const statusBar = document.createElement("div");
  statusBar.id = "status-bar";
  statusBar.textContent = "Ready";

  app.appendChild(toolbar);
  app.appendChild(breadcrumb);
  app.appendChild(main);
  app.appendChild(statusBar);

  return { toolbar, breadcrumb, sidebar, canvasContainer, canvasEl, statusBar };
}

/** Update the breadcrumb navigation bar. Pass empty array to clear/hide. */
export function updateBreadcrumb(labels: string[]): void {
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
  breadcrumb.appendChild(root);

  for (const label of labels) {
    const sep = document.createElement("span");
    sep.className = "bc-sep";
    sep.textContent = "›";
    breadcrumb.appendChild(sep);

    const item = document.createElement("span");
    item.className = "bc-item";
    item.textContent = label;
    breadcrumb.appendChild(item);
  }
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

export function buildToolbar(
  toolbar: HTMLElement,
  canvas: ZihuanCanvas,
  statusBar: HTMLElement,
  onNewGraph: () => void,
  onOpenFile: () => void,
  onUpload: () => void,
  onSaveFile: () => void,
  onValidate: () => void,
  onExecute: () => void,
  onStopTask: () => void
): void {
  // "Exit subgraph" button — visible only when inside a subgraph
  const exitSubgraphBtn = document.createElement("button");
  exitSubgraphBtn.textContent = "← 退出子图";
  exitSubgraphBtn.id = "btn-exit-subgraph";
  exitSubgraphBtn.style.display = "none";
  exitSubgraphBtn.addEventListener("click", () => {
    canvas.exitSubgraph().catch((e: Error) => {
      statusBar.textContent = `Exit subgraph error: ${e.message}`;
    });
  });
  toolbar.appendChild(exitSubgraphBtn);

  const buttons: Array<{ label: string; id?: string; danger?: boolean; onClick: () => void }> = [
    { label: "New", onClick: onNewGraph },
    { label: "Open...", onClick: onOpenFile },
    { label: "Upload", onClick: onUpload },
    { label: "Save", onClick: onSaveFile },
    { label: "Validate", onClick: onValidate },
    { label: "Run ▶", onClick: onExecute },
    { label: "Stop ■", danger: true, id: "btn-stop", onClick: onStopTask },
  ];

  for (const btn of buttons) {
    const el = document.createElement("button");
    el.textContent = btn.label;
    if (btn.danger) el.classList.add("danger");
    if (btn.id) el.id = btn.id;
    el.addEventListener("click", btn.onClick);
    toolbar.appendChild(el);
  }

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
