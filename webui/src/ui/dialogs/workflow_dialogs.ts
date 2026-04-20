import { ensureDialogStyles, openOverlay } from "./base";
import type { WorkflowEntry } from "./types";
import "./workflow_dialogs.css";

function ensureBrowserStyles(): void {
  // Styles are injected via CSS import (workflow_dialogs.css)
}

export function showSaveAsDialog(currentName: string): Promise<"local" | "workflow" | null> {
  return new Promise((resolve) => {
    const { dialog, close } = openOverlay();
    dialog.style.minWidth = "320px";
    dialog.style.maxWidth = "420px";

    const title = document.createElement("h3");
    title.textContent = "另存为";
    dialog.appendChild(title);

    const hint = document.createElement("p");
    hint.style.cssText = "margin:0 0 16px;font-size:13px;color:#aaa;";
    hint.textContent = `当前文件: ${currentName}`;
    dialog.appendChild(hint);

    const buttons = document.createElement("div");
    buttons.className = "zh-buttons";
    buttons.style.flexDirection = "column";
    buttons.style.gap = "8px";

    const localBtn = document.createElement("button");
    localBtn.textContent = "下载到本地";
    localBtn.style.width = "100%";
    localBtn.addEventListener("click", () => {
      close();
      resolve("local");
    });

    const workflowBtn = document.createElement("button");
    workflowBtn.textContent = "保存到工作流集";
    workflowBtn.style.width = "100%";
    workflowBtn.addEventListener("click", () => {
      close();
      resolve("workflow");
    });

    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.style.cssText = "width:100%;background:transparent;border-color:#555;color:#aaa;";
    cancelBtn.addEventListener("click", () => {
      close();
      resolve(null);
    });

    buttons.appendChild(workflowBtn);
    buttons.appendChild(localBtn);
    buttons.appendChild(cancelBtn);
    dialog.appendChild(buttons);
  });
}

export function showWorkflowsDialog(files: string[]): Promise<string | null> {
  ensureDialogStyles();

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-dialog";
    dialog.style.minWidth = "320px";
    dialog.style.maxWidth = "480px";

    const title = document.createElement("h3");
    title.textContent = "打开 Workflow";
    dialog.appendChild(title);

    const list = document.createElement("div");
    list.style.cssText = "display:flex;flex-direction:column;gap:4px;max-height:50vh;overflow-y:auto;margin-bottom:12px;";

    for (const file of files) {
      const item = document.createElement("div");
      item.style.cssText =
        "padding:8px 12px;border:1px solid #2a2a4a;border-radius:4px;cursor:pointer;transition:background 0.1s;font-size:13px;";
      item.textContent = file;
      item.addEventListener("mouseenter", () => {
        item.style.background = "#1a3a6e";
      });
      item.addEventListener("mouseleave", () => {
        item.style.background = "";
      });
      item.addEventListener("click", () => {
        document.body.removeChild(overlay);
        resolve(file);
      });
      list.appendChild(item);
    }

    dialog.appendChild(list);

    const buttons = document.createElement("div");
    buttons.className = "zh-buttons";

    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => {
      document.body.removeChild(overlay);
      resolve(null);
    });
    buttons.appendChild(cancelBtn);
    dialog.appendChild(buttons);

    overlay.appendChild(dialog);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) {
        document.body.removeChild(overlay);
        resolve(null);
      }
    });

    document.body.appendChild(overlay);
  });
}

export function showWorkflowBrowserDialog(workflows: WorkflowEntry[]): Promise<string | null> {
  ensureBrowserStyles();

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-wf-browser-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-wf-browser-dialog";
    dialog.addEventListener("click", (e) => e.stopPropagation());

    const header = document.createElement("div");
    header.className = "zh-wf-browser-header";
    const title = document.createElement("h3");
    title.textContent = "浏览工作流集";
    const closeBtn = document.createElement("button");
    closeBtn.className = "zh-wf-browser-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭";
    const close = () => {
      overlay.remove();
      resolve(null);
    };
    closeBtn.addEventListener("click", close);
    header.appendChild(title);
    header.appendChild(closeBtn);
    dialog.appendChild(header);

    const searchBar = document.createElement("div");
    searchBar.className = "zh-wf-browser-search";
    const searchInput = document.createElement("input");
    searchInput.type = "search";
    searchInput.placeholder = "搜索工作流名称、文件名、描述…";
    searchInput.autocomplete = "off";
    searchBar.appendChild(searchInput);
    dialog.appendChild(searchBar);

    const grid = document.createElement("div");
    grid.className = "zh-wf-browser-grid";

    function buildCard(wf: WorkflowEntry): HTMLElement {
      const card = document.createElement("div");
      card.className = "zh-wf-card";
      const cardTitle = wf.display_name || wf.name;
      card.title = [cardTitle, wf.file, wf.description].filter(Boolean).join("\n");

      const coverDiv = document.createElement("div");
      coverDiv.className = "zh-wf-card-cover";

      if (wf.cover_url) {
        const img = document.createElement("img");
        img.src = wf.cover_url;
        img.alt = cardTitle;
        img.draggable = false;
        coverDiv.appendChild(img);
      } else {
        const placeholder = document.createElement("span");
        placeholder.className = "zh-wf-no-cover";
        placeholder.textContent = "⬡";
        coverDiv.appendChild(placeholder);
      }

      const nameDiv = document.createElement("div");
      nameDiv.className = "zh-wf-card-name";
      nameDiv.textContent = cardTitle;

      const fileDiv = document.createElement("div");
      fileDiv.className = "zh-wf-card-filename";
      fileDiv.textContent = wf.file;

      card.appendChild(coverDiv);
      card.appendChild(nameDiv);
      card.appendChild(fileDiv);

      if (wf.description) {
        const descDiv = document.createElement("div");
        descDiv.className = "zh-wf-card-meta";
        descDiv.textContent = wf.description;
        card.appendChild(descDiv);
      }

      if (wf.version) {
        const verDiv = document.createElement("div");
        verDiv.className = "zh-wf-card-version";
        verDiv.textContent = `v${wf.version}`;
        card.appendChild(verDiv);
      }

      card.addEventListener("click", () => {
        overlay.remove();
        resolve(wf.file);
      });

      return card;
    }

    const emptyEl = document.createElement("div");
    emptyEl.className = "zh-wf-empty";

    function renderGrid(query: string): void {
      grid.innerHTML = "";
      const q = query.trim().toLowerCase();
      const filtered = q
        ? workflows.filter((wf) => {
            const haystack = [wf.display_name, wf.name, wf.file, wf.description, wf.version]
              .filter(Boolean)
              .join(" ")
              .toLowerCase();
            let pos = 0;
            for (const ch of q) {
              const idx = haystack.indexOf(ch, pos);
              if (idx === -1) return false;
              pos = idx + 1;
            }
            return true;
          })
        : workflows;

      if (filtered.length === 0) {
        emptyEl.textContent = q
          ? `没有匹配 "${query}" 的工作流`
          : "workflow_set/ 目录中没有工作流文件";
        grid.appendChild(emptyEl);
      } else {
        for (const wf of filtered) {
          grid.appendChild(buildCard(wf));
        }
      }
    }

    renderGrid("");
    searchInput.addEventListener("input", () => renderGrid(searchInput.value));

    dialog.appendChild(grid);
    overlay.appendChild(dialog);

    overlay.addEventListener("click", close);
    document.body.appendChild(overlay);
    setTimeout(() => searchInput.focus(), 0);
  });
}
