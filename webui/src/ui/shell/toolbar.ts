import { ws } from "../../api/ws";

export function buildToolbar(
  toolbar: HTMLElement,
  onNewGraph: () => void,
  onOpenFile: () => void,
  onSaveFile: () => void,
  onSaveAs: () => void,
  onSaveToWorkflows: () => void,
  onValidate: () => void,
  onBrowseWorkflows: () => void,
  onEditGraphInfo: () => void,
  onTaskFailed: (message: string) => void,
  onUndo: () => void,
  onRedo: () => void,
): { updateUndoRedoButtons: (canUndo: boolean, canRedo: boolean) => void } {
  const titleEl = toolbar.querySelector<HTMLElement>(".title")!;

  const menu = document.createElement("div");
  menu.id = "toolbar-menu";

  const menuItems: Array<{ label: string; shortcut?: string; onClick: () => void; separator?: boolean }> = [
    { label: "新建", shortcut: "Ctrl+N", onClick: onNewGraph },
    { label: "打开...", shortcut: "Ctrl+O", onClick: onOpenFile },
    { label: "保存", shortcut: "Ctrl+S", onClick: onSaveFile },
    { label: "另存为", shortcut: "Ctrl+Shift+S", onClick: onSaveAs },
    { label: "撤回", shortcut: "Ctrl+Z", onClick: onUndo, separator: true },
    { label: "重做", shortcut: "Ctrl+Y", onClick: onRedo },
    { label: "保存为工作流集", onClick: onSaveToWorkflows, separator: true },
    { label: "浏览工作流集", onClick: onBrowseWorkflows },
    { label: "编辑节点图信息...", onClick: onEditGraphInfo },
    { label: "验证", onClick: onValidate },
  ];

  for (const item of menuItems) {
    if (item.separator) {
      const sep = document.createElement("div");
      sep.className = "menu-separator";
      menu.appendChild(sep);
    }
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

  // Undo / Redo buttons
  const undoBtn = document.createElement("button");
  undoBtn.id = "toolbar-undo";
  undoBtn.className = "toolbar-icon-btn";
  undoBtn.title = "撤回 (Ctrl+Z)";
  undoBtn.textContent = "↩";
  undoBtn.disabled = true;
  undoBtn.addEventListener("click", () => onUndo());
  toolbar.appendChild(undoBtn);

  const redoBtn = document.createElement("button");
  redoBtn.id = "toolbar-redo";
  redoBtn.className = "toolbar-icon-btn";
  redoBtn.title = "重做 (Ctrl+Y)";
  redoBtn.textContent = "↪";
  redoBtn.disabled = true;
  redoBtn.addEventListener("click", () => onRedo());
  toolbar.appendChild(redoBtn);

  const spacer = document.createElement("span");
  spacer.className = "spacer";
  toolbar.appendChild(spacer);

  const taskStatus = document.createElement("span");
  taskStatus.className = "task-status";
  taskStatus.id = "task-status";
  taskStatus.textContent = "Idle";
  toolbar.appendChild(taskStatus);

  ws.onMessage((msg) => {
    if (msg.type === "TaskStarted") {
      taskStatus.textContent = `Running: ${msg.graph_name}`;
      taskStatus.className = "task-status running";
    } else if (msg.type === "TaskFinished") {
      const success = msg.success;
      taskStatus.textContent = success ? "Done ✓" : "Failed ✗";
      taskStatus.className = "task-status";
      if (!success) {
        onTaskFailed(msg.error ? `执行失败: ${msg.error}` : "执行失败");
      }
      setTimeout(() => {
        taskStatus.textContent = "Idle";
      }, 5000);
    } else if (msg.type === "TaskStopped") {
      taskStatus.textContent = "Stopped";
      taskStatus.className = "task-status";
      setTimeout(() => {
        taskStatus.textContent = "Idle";
      }, 3000);
    }
  });

  return {
    updateUndoRedoButtons(canUndo: boolean, canRedo: boolean): void {
      undoBtn.disabled = !canUndo;
      redoBtn.disabled = !canRedo;
    },
  };
}
