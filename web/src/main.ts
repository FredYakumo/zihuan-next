// Zihuan Next — Web UI entry point

import { registry, graphs, fileIO, tasks } from "./api/client";
import { ws } from "./api/ws";
import { registerNodeTypes } from "./graph/registry";
import { ZihuanCanvas } from "./graph/canvas";
import { injectStyles, buildDOM, buildSidebar, buildToolbar, updateBreadcrumb } from "./ui/shell";
import type { NodeTypeInfo } from "./api/types";

async function main() {
  injectStyles();
  // breadcrumb is appended to the DOM by buildDOM; updateBreadcrumb() accesses it by id
  const { toolbar, sidebar, canvasContainer, canvasEl, statusBar } = buildDOM();

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

  // Wire breadcrumb navigation
  canvas.onNavigationChange = (labels) => {
    updateBreadcrumb(labels);
    const exitBtn = document.getElementById("btn-exit-subgraph") as HTMLButtonElement | null;
    if (exitBtn) exitBtn.style.display = labels.length > 0 ? "" : "none";
    if (labels.length > 0) {
      statusBar.textContent = `子图: ${labels[labels.length - 1]}`;
    }
  };

  // Build sidebar
  buildSidebar(
    sidebar,
    nodeTypes,
    async (typeId, x, y) => {
      const sid = canvas.sessionId;
      if (!sid) {
        alert("No graph open. Create or open a graph first.");
        return;
      }
      try {
        await graphs.addNode(sid, typeId, undefined, x, y);
        await canvas.loadSession(sid);
        statusBar.textContent = `Added node: ${typeId}`;
      } catch (e) {
        console.error("addNode error:", e);
        statusBar.textContent = `Error: ${(e as Error).message}`;
      }
    },
    canvasContainer
  );

  // State: current running task id
  let currentTaskId: string | null = null;
  ws.onMessage((msg) => {
    if (msg.type === "TaskStarted") currentTaskId = msg.task_id;
    if (msg.type === "TaskFinished" || msg.type === "TaskStopped") currentTaskId = null;
  });

  // Toolbar actions
  const onNewGraph = async () => {
    try {
      const tab = await graphs.create();
      await canvas.loadSession(tab.id);
      statusBar.textContent = `Created new graph (${tab.id.slice(0, 8)})`;
    } catch (e) {
      statusBar.textContent = `Error: ${(e as Error).message}`;
    }
  };

  const onOpenFile = async () => {
    const path = prompt("Enter server-side file path:");
    if (!path) return;
    try {
      const result = await fileIO.open(path);
      await canvas.loadSession(result.session_id);
      if (result.migrated) statusBar.textContent = "Graph loaded (port types migrated)";
      else statusBar.textContent = `Opened: ${path}`;
    } catch (e) {
      statusBar.textContent = `Error: ${(e as Error).message}`;
    }
  };

  const onUpload = async () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const result = await fileIO.upload(file);
        await canvas.loadSession(result.session_id);
        statusBar.textContent = `Uploaded: ${file.name}`;
      } catch (e) {
        statusBar.textContent = `Error: ${(e as Error).message}`;
      }
    };
    input.click();
  };

  const onSaveFile = async () => {
    const sid = canvas.sessionId;
    if (!sid) {
      statusBar.textContent = "No graph open";
      return;
    }
    try {
      const result = await graphs.saveFile(sid);
      statusBar.textContent = `Saved to: ${result.path}`;
    } catch (e) {
      // If no server path yet, offer download
      const url = graphs.downloadUrl(sid);
      const a = document.createElement("a");
      a.href = url;
      a.download = "graph.json";
      a.click();
      statusBar.textContent = "Downloading graph JSON...";
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
      currentTaskId = result.task_id;
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

  buildToolbar(
    toolbar,
    canvas,
    statusBar,
    onNewGraph,
    onOpenFile,
    onUpload,
    onSaveFile,
    onValidate,
    onExecute,
    onStopTask
  );

  // Auto-resize canvas to fill its container
  const resizeCanvas = () => {
    canvasEl.width = canvasContainer.clientWidth;
    canvasEl.height = canvasContainer.clientHeight;
  };
  resizeCanvas();
  window.addEventListener("resize", resizeCanvas);

  // Start position sync
  canvas.startPositionSync(3000);

  statusBar.textContent = "Ready — create or open a graph to begin";
}

main().catch(console.error);
