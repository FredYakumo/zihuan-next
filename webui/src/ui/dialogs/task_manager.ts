import type { TaskEntry, TaskLogEntry } from "../../api/types";
import { formatTaskDuration, formatTaskTimestamp, taskDisplayName, type TaskManagerStore } from "../../app/task_manager";
import { openOverlay, showErrorDialog } from "./base";

function makeBadge(level: string): HTMLElement {
  const badge = document.createElement("span");
  badge.className = `log-badge log-badge-${level.toUpperCase()}`;
  badge.textContent = level.toUpperCase();
  return badge;
}

function openTaskLogsDialog(task: TaskEntry, entries: TaskLogEntry[]): void {
  const { overlay, dialog, close } = openOverlay();
  dialog.classList.add("task-log-dialog");

  const titleRow = document.createElement("div");
  titleRow.className = "task-log-header";
  const title = document.createElement("h3");
  title.textContent = `${taskDisplayName(task)} · 任务日志`;
  titleRow.appendChild(title);

  const closeBtn = document.createElement("button");
  closeBtn.className = "task-log-close";
  closeBtn.textContent = "×";
  closeBtn.title = "关闭";
  closeBtn.addEventListener("click", close);
  titleRow.appendChild(closeBtn);
  dialog.appendChild(titleRow);

  const list = document.createElement("div");
  list.className = "task-log-list";
  if (entries.length === 0) {
    const empty = document.createElement("div");
    empty.className = "task-empty-state";
    empty.textContent = "这个任务暂时没有日志。";
    list.appendChild(empty);
  } else {
    for (const entry of entries) {
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
      list.appendChild(row);
    }
  }
  dialog.appendChild(list);

  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) close();
  });
}

export function openTaskManagerDialog(store: TaskManagerStore): void {
  const { overlay, dialog, close } = openOverlay();
  dialog.classList.add("task-manager-dialog");

  const header = document.createElement("div");
  header.className = "task-manager-header";

  const titleBlock = document.createElement("div");
  const title = document.createElement("h3");
  title.textContent = "任务管理器";
  titleBlock.appendChild(title);

  const subtitle = document.createElement("div");
  subtitle.className = "task-manager-subtitle";
  subtitle.textContent = "查看运行中的任务、历史记录、日志和重跑操作。";
  titleBlock.appendChild(subtitle);
  header.appendChild(titleBlock);

  const closeBtn = document.createElement("button");
  closeBtn.className = "task-manager-close";
  closeBtn.textContent = "×";
  closeBtn.title = "关闭";
  closeBtn.addEventListener("click", close);
  header.appendChild(closeBtn);
  dialog.appendChild(header);

  const body = document.createElement("div");
  body.className = "task-manager-body";
  dialog.appendChild(body);

  const footer = document.createElement("div");
  footer.className = "task-manager-footer";
  const clearBtn = document.createElement("button");
  clearBtn.className = "task-action-btn task-action-btn-clear";
  clearBtn.textContent = "清空未在运行的任务列表";
  clearBtn.addEventListener("click", async () => {
    try {
      await store.clearFinished();
    } catch (error) {
      showErrorDialog(`清空任务列表失败: ${(error as Error).message}`);
    }
  });
  footer.appendChild(clearBtn);
  dialog.appendChild(footer);

  let currentTasks = store.getTasks();

  const render = (): void => {
    body.innerHTML = "";
    if (currentTasks.length === 0) {
      const empty = document.createElement("div");
      empty.className = "task-empty-state";
      empty.textContent = "当前还没有任务记录。";
      body.appendChild(empty);
      return;
    }

    const table = document.createElement("div");
    table.className = "task-table";

    const head = document.createElement("div");
    head.className = "task-row task-row-head";
    for (const label of ["节点图", "开始时间", "运行时长", "用户 IP", "操作"]) {
      const cell = document.createElement("div");
      cell.className = "task-cell";
      cell.textContent = label;
      head.appendChild(cell);
    }
    table.appendChild(head);

    for (const task of currentTasks) {
      const row = document.createElement("div");
      row.className = "task-row";

      const nameCell = document.createElement("div");
      nameCell.className = "task-cell task-cell-name";
      const name = document.createElement("div");
      name.className = "task-name";
      name.textContent = taskDisplayName(task);
      nameCell.appendChild(name);
      if (task.error_message) {
        const err = document.createElement("div");
        err.className = "task-error-text";
        err.textContent = task.error_message;
        nameCell.appendChild(err);
      }
      row.appendChild(nameCell);

      const startCell = document.createElement("div");
      startCell.className = "task-cell";
      startCell.textContent = formatTaskTimestamp(task.start_time);
      row.appendChild(startCell);

      const durationCell = document.createElement("div");
      durationCell.className = "task-cell";
      durationCell.textContent = formatTaskDuration(task);
      row.appendChild(durationCell);

      const ipCell = document.createElement("div");
      ipCell.className = "task-cell";
      ipCell.textContent = task.user_ip ?? "-";
      row.appendChild(ipCell);

      const actionCell = document.createElement("div");
      actionCell.className = "task-cell task-cell-actions";

      const logBtn = document.createElement("button");
      logBtn.className = "task-action-btn";
      logBtn.textContent = "日志";
      logBtn.addEventListener("click", async () => {
        try {
          const entries = await store.getTaskLogs(task.id);
          openTaskLogsDialog(task, entries);
        } catch (error) {
          showErrorDialog(`读取任务日志失败: ${(error as Error).message}`);
        }
      });
      actionCell.appendChild(logBtn);

      if (task.is_running) {
        const stopBtn = document.createElement("button");
        stopBtn.className = "task-action-btn danger";
        stopBtn.textContent = "结束";
        stopBtn.addEventListener("click", async () => {
          try {
            await store.stopTask(task.id);
          } catch (error) {
            showErrorDialog(`结束任务失败: ${(error as Error).message}`);
          }
        });
        actionCell.appendChild(stopBtn);
      } else {
        const rerunBtn = document.createElement("button");
        rerunBtn.className = "task-action-btn primary";
        rerunBtn.textContent = "重新运行";
        rerunBtn.disabled = !task.can_rerun;
        rerunBtn.title = task.can_rerun ? "重新运行该任务" : "未保存的任务无法重新运行";
        rerunBtn.addEventListener("click", async () => {
          try {
            await store.rerunTask(task.id);
          } catch (error) {
            showErrorDialog(`重新运行任务失败: ${(error as Error).message}`);
          }
        });
        actionCell.appendChild(rerunBtn);
      }

      row.appendChild(actionCell);
      table.appendChild(row);
    }

    body.appendChild(table);
  };

  const unsubscribe = store.subscribe((tasks) => {
    currentTasks = tasks;
    render();
  });
  const timer = window.setInterval(render, 1000);
  render();

  const cleanup = (): void => {
    window.clearInterval(timer);
    unsubscribe();
  };

  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) {
      cleanup();
      close();
    }
  });
  closeBtn.addEventListener("click", cleanup, { once: true });
}
