import type { TaskEntry } from "../../api/types";
import { getThemeConfig } from "../theme";
import { openOverlay } from "../dialogs/base";

function taskLabel(task: Pick<TaskEntry, "graph_name" | "is_workflow_set">): string {
  return task.is_workflow_set ? `${task.graph_name} [工作流集]` : task.graph_name;
}

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
  onEditGraphIO: () => void,
  onOpenTaskManager: () => void,
  onStopTask: (taskId: string) => void,
  onBackToConsole: () => void,
  onUndo: () => void,
  onRedo: () => void,
  getThemeNames: () => Array<{
    name: string;
    display_name: string;
    class_name: string;
    schema: "light" | "dark";
    mode: string;
  }>,
  getCurrentThemeName: () => string,
  onSwitchTheme: (name: string) => void,
): {
  updateUndoRedoButtons: (canUndo: boolean, canRedo: boolean) => void;
  setTaskState: (tasks: TaskEntry[]) => void;
} {
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
    { label: "编辑节点图输入/输出...", onClick: onEditGraphIO },
    { label: "验证", onClick: onValidate },
    { label: "返回控制台", onClick: onBackToConsole, separator: true },
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

  // ─── Theme picker dialog ──────────────────────────────────────────────────────

  const themeSep = document.createElement("div");
  themeSep.className = "menu-separator";
  menu.appendChild(themeSep);

  const themeItem = document.createElement("div");
  themeItem.className = "menu-item";
  themeItem.innerHTML = `<span>主题</span>`;
  themeItem.addEventListener("click", () => {
    menu.classList.remove("open");
    openThemeDialog();
  });
  menu.appendChild(themeItem);

  // Theme preview tooltip (shared, positioned fixed)
  const themePreview = document.createElement("div");
  themePreview.id = "theme-preview";
  themePreview.style.cssText =
    "position:fixed;width:180px;height:130px;border-radius:6px;" +
    "border:1px solid var(--border);box-shadow:0 6px 20px rgba(0,0,0,0.5);" +
    "z-index:10002;display:none;overflow:hidden;font-size:11px;" +
    "font-family:sans-serif;pointer-events:none;";
  themePreview.innerHTML = `
    <div id="tp-toolbar" style="height:24px;display:flex;align-items:center;padding:0 8px;gap:4px;">
      <span style="font-weight:bold;">预览</span>
      <span class="tp-text-muted" style="margin-left:auto;">Aa</span>
    </div>
    <div id="tp-body" style="padding:8px;display:flex;flex-direction:column;gap:6px;">
      <div class="tp-text">主文字颜色</div>
      <div style="display:flex;gap:6px;align-items:center;">
        <div id="tp-accent" style="width:16px;height:16px;border-radius:3px;"></div>
        <span class="tp-text-muted">强调色</span>
      </div>
      <div id="tp-btn" style="padding:3px 10px;border-radius:3px;display:inline-block;width:fit-content;">按钮</div>
    </div>
  `;
  document.body.appendChild(themePreview);

  function openThemeDialog(): void {
    const { overlay, dialog, close } = openOverlay();
    dialog.style.minWidth = "320px";
    dialog.style.maxWidth = "420px";

    const title = document.createElement("h3");
    title.textContent = "选择主题";
    dialog.appendChild(title);

    const current = getCurrentThemeName();
    const list = document.createElement("div");
    list.style.cssText = "display:flex;flex-direction:column;gap:4px;margin-top:8px;";

    for (const t of getThemeNames()) {
      const row = document.createElement("div");
      row.className = "menu-item";
      row.style.cssText =
        "padding:8px 12px;border-radius:4px;cursor:pointer;" +
        "display:flex;align-items:center;justify-content:space-between;" +
        "border:1px solid transparent;transition:border-color 0.12s,background 0.12s;";
      row.innerHTML = `
        <span style="display:flex;align-items:center;gap:8px;">
          <span class="tp-color-dot" style="width:12px;height:12px;border-radius:50%;display:inline-block;background:${getThemeAccentColor(t.name)};"></span>
          <span>${t.display_name}</span>
        </span>
      `;
      if (t.name === current) {
        const check = document.createElement("span");
        check.textContent = "✓";
        check.style.color = "var(--run-color)";
        row.appendChild(check);
      }

      row.addEventListener("mouseenter", () => {
        row.style.background = "var(--node-hover)";
        row.style.borderColor = "var(--border)";
        showPreview(t.name, row);
      });
      row.addEventListener("mouseleave", () => {
        row.style.background = "transparent";
        row.style.borderColor = "transparent";
        hidePreview();
      });
      row.addEventListener("click", () => {
        hidePreview();
        onSwitchTheme(t.name);
        close();
      });

      list.appendChild(row);
    }
    dialog.appendChild(list);

    const btns = document.createElement("div");
    btns.className = "zh-buttons";
    btns.style.marginTop = "16px";
    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => {
      hidePreview();
      close();
    });
    btns.appendChild(cancelBtn);
    dialog.appendChild(btns);

    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) {
        hidePreview();
        close();
      }
    });
  }

  function getThemeAccentColor(name: string): string {
    const config = getThemeConfig(name);
    return config?.css["--accent"] ?? "#3b82f6";
  }

  function showPreview(name: string, anchor: HTMLElement): void {
    const config = getThemeConfig(name);
    if (!config) return;
    const css = config.css;
    const lg = config.litegraph;

    const rect = anchor.getBoundingClientRect();
    // Position preview to the right of the dialog row, or above if no space
    let left = rect.right + 12;
    let top = rect.top;
    if (left + 180 > window.innerWidth) {
      left = rect.left - 192;
    }
    if (top + 130 > window.innerHeight) {
      top = window.innerHeight - 140;
    }
    themePreview.style.left = `${left}px`;
    themePreview.style.top = `${top}px`;
    themePreview.style.display = "block";

    const tpToolbar = themePreview.querySelector<HTMLElement>("#tp-toolbar")!;
    const tpBody = themePreview.querySelector<HTMLElement>("#tp-body")!;
    const tpAccent = themePreview.querySelector<HTMLElement>("#tp-accent")!;
    const tpBtn = themePreview.querySelector<HTMLElement>("#tp-btn")!;

    themePreview.style.background = css["--bg"] ?? lg.canvasBg ?? "#0d0d0d";
    themePreview.style.color = css["--text"] ?? lg.nodeTitleText ?? "#e6e6e6";
    tpToolbar.style.background = css["--toolbar-bg"] ?? lg.nodeHeader ?? "#1a1a1a";
    tpBody.style.background = css["--bg"] ?? lg.canvasBg ?? "#0d0d0d";
    tpAccent.style.background = css["--accent"] ?? "#3b82f6";
    tpBtn.style.background = css["--btn-primary"] ?? "#2563eb";
    tpBtn.style.color = css["--btn-primary-text"] ?? "#ffffff";

    themePreview.querySelectorAll(".tp-text").forEach((el) => {
      (el as HTMLElement).style.color = css["--text"] ?? "#e6e6e6";
    });
    themePreview.querySelectorAll(".tp-text-muted").forEach((el) => {
      (el as HTMLElement).style.color = css["--text-muted"] ?? "#a0a0a0";
    });
  }

  function hidePreview(): void {
    themePreview.style.display = "none";
  }

  toolbar.appendChild(menu);

  titleEl.addEventListener("click", (e) => {
    e.stopPropagation();
    menu.classList.toggle("open");
    hidePreview();
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

  const backToConsoleBtn = document.createElement("button");
  backToConsoleBtn.id = "toolbar-back-console";
  backToConsoleBtn.className = "toolbar-nav-btn";
  backToConsoleBtn.type = "button";
  backToConsoleBtn.title = "控制台";
  backToConsoleBtn.textContent = "控制台";
  backToConsoleBtn.addEventListener("click", () => onBackToConsole());

  const spacer = document.createElement("span");
  spacer.className = "spacer";
  toolbar.appendChild(spacer);

  toolbar.appendChild(backToConsoleBtn);

  const taskStatusWrap = document.createElement("div");
  taskStatusWrap.className = "task-status-wrap";

  const taskStatus = document.createElement("button");
  taskStatus.className = "task-status";
  taskStatus.id = "task-status";
  taskStatus.textContent = "Idle";
  taskStatus.type = "button";
  taskStatus.title = "查看任务状态";
  taskStatus.addEventListener("click", onOpenTaskManager);
  taskStatusWrap.appendChild(taskStatus);

  const taskPopover = document.createElement("div");
  taskPopover.className = "task-status-popover";
  taskStatusWrap.appendChild(taskPopover);
  toolbar.appendChild(taskStatusWrap);

  let latestTasks: TaskEntry[] = [];
  let hideTimer: number | null = null;

  const cancelHide = (): void => {
    if (hideTimer !== null) {
      window.clearTimeout(hideTimer);
      hideTimer = null;
    }
  };

  const closePopover = (): void => {
    cancelHide();
    taskPopover.classList.remove("open");
  };

  const scheduleHide = (): void => {
    cancelHide();
    hideTimer = window.setTimeout(() => {
      taskPopover.classList.remove("open");
    }, 120);
  };

  const renderTaskPopover = (): void => {
    taskPopover.innerHTML = "";
    const runningTasks = latestTasks.filter((task) => task.is_running);
    if (runningTasks.length === 0) {
      return;
    }

    const header = document.createElement("div");
    header.className = "task-popover-header";
    header.textContent = "正在运行的任务";
    taskPopover.appendChild(header);

    for (const task of runningTasks.slice(0, 5)) {
      const row = document.createElement("div");
      row.className = "task-popover-row";

      const name = document.createElement("span");
      name.className = "task-popover-name";
      name.textContent = taskLabel(task);
      row.appendChild(name);

      const stopBtn = document.createElement("button");
      stopBtn.type = "button";
      stopBtn.className = "task-popover-stop";
      stopBtn.textContent = "结束";
      stopBtn.addEventListener("click", (event) => {
        event.stopPropagation();
        onStopTask(task.id);
      });
      row.appendChild(stopBtn);
      taskPopover.appendChild(row);
    }

    if (runningTasks.length > 5) {
      const more = document.createElement("div");
      more.className = "task-popover-more";
      more.textContent = `还有 ${runningTasks.length - 5} 个任务未显示`;
      taskPopover.appendChild(more);
    }

    const openBtn = document.createElement("button");
    openBtn.type = "button";
    openBtn.className = "task-popover-open";
    openBtn.textContent = "打开任务管理器";
    openBtn.addEventListener("click", (event) => {
      event.stopPropagation();
      closePopover();
      onOpenTaskManager();
    });
    taskPopover.appendChild(openBtn);
  };

  const renderTaskStatus = (): void => {
    const runningTasks = latestTasks.filter((task) => task.is_running);
    if (runningTasks.length === 0) {
      taskStatus.textContent = "Idle";
      taskStatus.className = "task-status";
      closePopover();
      return;
    }

    taskStatus.className = "task-status running";
    taskStatus.textContent = runningTasks.length === 1
      ? `运行中: ${taskLabel(runningTasks[0])}`
      : `运行中: ${runningTasks.length} 个任务`;
    renderTaskPopover();
  };

  taskStatusWrap.addEventListener("mouseenter", () => {
    cancelHide();
    if (latestTasks.some((task) => task.is_running)) {
      taskPopover.classList.add("open");
    }
  });
  taskStatusWrap.addEventListener("mouseleave", scheduleHide);

  renderTaskStatus();

  return {
    updateUndoRedoButtons(canUndo: boolean, canRedo: boolean): void {
      undoBtn.disabled = !canUndo;
      redoBtn.disabled = !canRedo;
    },
    setTaskState(tasks: TaskEntry[]): void {
      latestTasks = [...tasks];
      renderTaskStatus();
    },
  };
}
