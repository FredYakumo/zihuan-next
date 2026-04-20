function makeBadge(level: string): HTMLElement {
  const badge = document.createElement("span");
  badge.className = `log-badge log-badge-${level.toUpperCase()}`;
  badge.textContent = level.toUpperCase();
  return badge;
}

export function buildCanvasPanelButtons(
  canvasContainer: HTMLElement,
  onHyperparameters: () => void,
  onVariables: () => void,
  onAddNode: () => void,
  onExecute: () => void,
  onStopTask: () => void,
): {
  updateRunButton: (isRunning: boolean) => void;
  appendLogEntry: (level: string, message: string, timestamp: string) => void;
} {
  const panel = document.createElement("div");
  panel.id = "canvas-panel-buttons";

  const addNodeBtn = document.createElement("button");
  addNodeBtn.textContent = "新建节点";
  addNodeBtn.title = "添加节点到画布";
  addNodeBtn.addEventListener("click", onAddNode);
  panel.appendChild(addNodeBtn);

  const hpBtn = document.createElement("button");
  hpBtn.textContent = "超参数";
  hpBtn.title = "管理超参数定义和值";
  hpBtn.addEventListener("click", onHyperparameters);
  panel.appendChild(hpBtn);

  const varBtn = document.createElement("button");
  varBtn.textContent = "变量";
  varBtn.title = "管理图变量";
  varBtn.addEventListener("click", onVariables);
  panel.appendChild(varBtn);

  const MAX_BUFFER = 1000;
  const logBuffer: Array<{ level: string; message: string; timestamp: string }> = [];
  let logOverlay: HTMLElement | null = null;
  let logList: HTMLElement | null = null;

  function buildLogEntry(entry: { level: string; message: string; timestamp: string }): HTMLElement {
    const row = document.createElement("div");
    row.className = "log-entry";
    row.appendChild(makeBadge(entry.level));
    const ts = document.createElement("span");
    ts.className = "log-entry-ts";
    ts.textContent = entry.timestamp;
    row.appendChild(ts);
    const msg = document.createElement("span");
    msg.className = "log-entry-msg";
    msg.textContent = entry.message;
    row.appendChild(msg);
    return row;
  }

  function closeLogOverlay(): void {
    if (logOverlay && logOverlay.parentNode) {
      logOverlay.parentNode.removeChild(logOverlay);
    }
    logOverlay = null;
    logList = null;
  }

  function openLogOverlay(): void {
    if (logOverlay) return;

    const overlay = document.createElement("div");
    overlay.className = "log-stream-overlay";
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) closeLogOverlay();
    });

    const dialog = document.createElement("div");
    dialog.className = "log-stream-dialog";
    dialog.addEventListener("click", (e) => e.stopPropagation());

    const header = document.createElement("div");
    header.className = "log-stream-header";
    const title = document.createElement("h3");
    title.textContent = "实时日志流";
    header.appendChild(title);
    const closeBtn = document.createElement("button");
    closeBtn.className = "log-stream-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭";
    closeBtn.addEventListener("click", closeLogOverlay);
    header.appendChild(closeBtn);
    dialog.appendChild(header);

    const list = document.createElement("div");
    list.className = "log-stream-list";
    for (const entry of logBuffer) {
      list.appendChild(buildLogEntry(entry));
    }
    dialog.appendChild(list);

    overlay.appendChild(dialog);
    document.body.appendChild(overlay);

    logOverlay = overlay;
    logList = list;

    requestAnimationFrame(() => {
      list.scrollTop = list.scrollHeight;
    });
  }

  const logBtn = document.createElement("button");
  logBtn.textContent = "日志";
  logBtn.title = "打开实时日志流";
  logBtn.addEventListener("click", openLogOverlay);
  panel.appendChild(logBtn);

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

  function appendLogEntry(level: string, message: string, timestamp: string): void {
    logBuffer.push({ level, message, timestamp });
    if (logBuffer.length > MAX_BUFFER) logBuffer.shift();

    if (logList) {
      logList.appendChild(buildLogEntry({ level, message, timestamp }));
      logList.scrollTop = logList.scrollHeight;
    }
  }

  return { updateRunButton, appendLogEntry };
}

export function createLogToastOverlay(
  canvasContainer: HTMLElement,
): (level: string, message: string) => void {
  const overlay = document.createElement("div");
  overlay.className = "log-toast-overlay";
  canvasContainer.appendChild(overlay);

  const MAX_TOASTS = 5;
  const FADE_AFTER_MS = 4200;
  const REMOVE_AFTER_MS = 5000;

  return function addLog(level: string, message: string): void {
    while (overlay.childElementCount >= MAX_TOASTS) {
      if (overlay.firstChild) overlay.removeChild(overlay.firstChild);
    }

    const toast = document.createElement("div");
    toast.className = "log-toast";

    toast.appendChild(makeBadge(level));

    const msgSpan = document.createElement("span");
    msgSpan.className = "log-toast-msg";
    msgSpan.textContent = message;
    toast.appendChild(msgSpan);

    overlay.appendChild(toast);

    setTimeout(() => toast.classList.add("fading"), FADE_AFTER_MS);
    setTimeout(() => {
      if (toast.parentNode) overlay.removeChild(toast);
    }, REMOVE_AFTER_MS);
  };
}
