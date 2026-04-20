import { ensureDialogStyles, openOverlay } from "./base";
import type { WorkflowEntry } from "./types";

const BROWSER_STYLES = `
  .zh-wf-browser-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.55);
    display: flex; align-items: center; justify-content: center;
    z-index: 9999; font-family: sans-serif;
  }
  .zh-wf-browser-dialog {
    background: var(--toolbar-bg); border: 1px solid var(--border); border-radius: 10px;
    width: 860px; max-width: 95vw; max-height: 85vh;
    display: flex; flex-direction: column;
    box-shadow: 0 12px 40px rgba(0,0,0,0.45); color: var(--text);
  }
  .zh-wf-browser-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 20px; border-bottom: 1px solid var(--border); flex-shrink: 0;
  }
  .zh-wf-browser-header h3 { margin: 0; font-size: 16px; color: var(--link); }
  .zh-wf-browser-close {
    background: transparent; border: none; color: var(--text-muted); font-size: 22px;
    cursor: pointer; padding: 0 4px; line-height: 1;
  }
  .zh-wf-browser-close:hover { color: var(--accent); }
  .zh-wf-browser-search {
    padding: 10px 20px; border-bottom: 1px solid var(--border); flex-shrink: 0;
  }
  .zh-wf-browser-search input {
    width: 100%; box-sizing: border-box; padding: 7px 12px;
    background: var(--input-bg); border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 13px; outline: none;
  }
  .zh-wf-browser-search input:focus { border-color: var(--link); }
  .zh-wf-browser-grid {
    flex: 1; overflow-y: auto; padding: 16px 20px;
    display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 14px; align-content: start;
  }
  .zh-wf-card {
    border: 1px solid var(--border); border-radius: 8px; overflow: hidden;
    cursor: pointer; transition: border-color 0.15s, transform 0.1s, box-shadow 0.15s;
    background: var(--bg); display: flex; flex-direction: column;
  }
  .zh-wf-card:hover {
    border-color: var(--link); transform: translateY(-2px);
    box-shadow: 0 6px 20px rgba(0,0,0,0.3);
  }
  .zh-wf-card-cover {
    width: 100%; aspect-ratio: 16/9; background: var(--input-bg);
    display: flex; align-items: center; justify-content: center;
    overflow: hidden; flex-shrink: 0;
  }
  .zh-wf-card-cover img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .zh-wf-card-cover .zh-wf-no-cover {
    font-size: 36px; opacity: 0.25; user-select: none; color: var(--link);
  }
  .zh-wf-card-name {
    padding: 8px 10px 2px; font-size: 13px; color: var(--text);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    border-top: 1px solid var(--border); font-weight: 600;
  }
  .zh-wf-card-filename {
    padding: 0 10px 2px; font-size: 10px; color: var(--text-dim);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .zh-wf-card-meta {
    padding: 0 10px 8px; font-size: 11px; color: var(--text-muted);
    overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .zh-wf-card-version {
    padding: 0 10px 6px; font-size: 10px; color: var(--text-faint);
  }
  .zh-wf-empty {
    grid-column: 1 / -1; color: var(--text-muted); font-size: 14px;
    text-align: center; padding: 40px 0;
  }
`;

function ensureBrowserStyles(): void {
  if (document.getElementById("zh-wf-browser-styles")) return;
  const style = document.createElement("style");
  style.id = "zh-wf-browser-styles";
  style.textContent = BROWSER_STYLES;
  document.head.appendChild(style);
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
