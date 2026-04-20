import type { NodeDefinition } from "../../api/types";
import { ensureDialogStyles, openOverlay } from "./base";
import { isValidConnectionType } from "./shared";
import type { ConnectPortChoice, PortSelectOption } from "./types";
import "./connection_dialogs.css";

function ensureConnectPortStyles(): void {
  // Styles are injected via CSS import (connection_dialogs.css)
}

export function showConnectPortDialog(
  currentNodes: NodeDefinition[],
  sourceNodeId: string,
  sourcePortName: string,
  sourceType: string,
  isFromOutput: boolean,
): Promise<ConnectPortChoice | null> {
  ensureDialogStyles();
  ensureConnectPortStyles();

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-dialog zh-cp-dialog";
    dialog.style.cssText = "padding: 0 !important; overflow: hidden; display: flex; flex-direction: column; min-width: 520px; max-width: 700px; max-height: 82vh;";

    const header = document.createElement("div");
    header.className = "zh-cp-header";
    header.style.cssText = "padding: 20px 24px 14px; border-bottom: 1px solid var(--border); display: flex; flex-direction: column; gap: 6px; flex-shrink: 0;";

    const title = document.createElement("h3");
    title.textContent = "选择连接目标";
    header.appendChild(title);

    const sourceInfo = document.createElement("div");
    sourceInfo.className = "zh-cp-source-info";
    sourceInfo.innerHTML = `
      <span>来源端口:</span>
      <span class="zh-cp-badge">${sourcePortName}</span>
      <span class="zh-cp-badge">${sourceType}</span>
      <span style="color:var(--text-dim)">${isFromOutput ? "→ 寻找输入端口" : "← 寻找输出端口"}</span>
    `;
    header.appendChild(sourceInfo);
    dialog.appendChild(header);

    const searchInput = document.createElement("input");
    searchInput.type = "text";
    searchInput.className = "zh-cp-search";
    searchInput.placeholder = "搜索节点或端口名称…";
    searchInput.style.cssText = "display: block; margin: 14px 24px 0; box-sizing: border-box; width: calc(100% - 48px); padding: 8px 12px; background: var(--input-bg); border: 1px solid var(--border); border-radius: 6px; color: var(--text); font-size: 13px; outline: none; flex-shrink: 0;";
    dialog.appendChild(searchInput);

    const listEl = document.createElement("div");
    listEl.className = "zh-cp-list";
    listEl.style.cssText = "flex: 1; overflow-y: auto; padding: 14px 24px 20px 24px; display: flex; flex-direction: column; gap: 5px; box-sizing: border-box;";
    dialog.appendChild(listEl);

    const footer = document.createElement("div");
    footer.className = "zh-cp-footer";
    footer.style.cssText = "padding: 12px 24px; border-top: 1px solid var(--border); display: flex; justify-content: flex-end; flex-shrink: 0;";
    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => {
      overlay.remove();
      resolve(null);
    });
    footer.appendChild(cancelBtn);
    dialog.appendChild(footer);

    overlay.appendChild(dialog);
    document.body.appendChild(overlay);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) {
        overlay.remove();
        resolve(null);
      }
    });

    const done = (choice: ConnectPortChoice) => {
      overlay.remove();
      resolve(choice);
    };

    function renderList(query: string) {
      listEl.innerHTML = "";
      const q = query.trim().toLowerCase();

      if (!q || "新建节点".includes(q)) {
        const newNodeBtn = document.createElement("div");
        newNodeBtn.className = "zh-cp-new-node";
        newNodeBtn.innerHTML = `<span style="font-size: 16px; margin-top: -2px; font-weight: normal;">+</span> <span>新建节点</span>`;
        newNodeBtn.addEventListener("click", () => done({ kind: "new_node" }));
        listEl.appendChild(newNodeBtn);
      }

      let anyNode = false;
      for (const node of currentNodes) {
        const nodeName = node.name || node.node_type;
        const ports = isFromOutput ? node.input_ports : node.output_ports;

        const compatiblePorts = ports.filter((p) => {
          if (p.hidden) return false;
          const pt = typeof p.data_type === "string"
            ? p.data_type
            : Object.keys(p.data_type as object).length > 0
              ? `${Object.keys(p.data_type as object)[0]}<${Object.values(p.data_type as object)[0] as string}>`
              : "*";
          if (!isValidConnectionType(sourceType, pt)) return false;
          if (q) {
            const portName = p.name.toLowerCase();
            const nodeNameLower = nodeName.toLowerCase();
            if (!portName.includes(q) && !nodeNameLower.includes(q)) return false;
          }
          return true;
        });

        if (compatiblePorts.length === 0) continue;
        anyNode = true;

        const nodeHeader = document.createElement("div");
        nodeHeader.className = "zh-cp-node-header";
        nodeHeader.textContent = nodeName;
        listEl.appendChild(nodeHeader);

        for (const p of compatiblePorts) {
          const pt = typeof p.data_type === "string"
            ? p.data_type
            : Object.keys(p.data_type as object).length > 0
              ? `${Object.keys(p.data_type as object)[0]}<${Object.values(p.data_type as object)[0] as string}>`
              : "*";

          const item = document.createElement("div");
          item.className = "zh-cp-port-item";
          const nameEl = document.createElement("span");
          nameEl.className = "zh-cp-port-name";
          nameEl.textContent = p.name;
          const badge = document.createElement("span");
          badge.className = "zh-cp-badge";
          badge.textContent = pt;
          item.appendChild(nameEl);
          item.appendChild(badge);
          item.addEventListener("click", () => done({ kind: "existing", targetNodeId: node.id, targetPortName: p.name }));
          listEl.appendChild(item);
        }
      }

      if (!anyNode && (!q || !"新建节点".includes(q))) {
        const empty = document.createElement("div");
        empty.className = "zh-cp-empty";
        empty.textContent = "没有找到兼容的端口";
        listEl.appendChild(empty);
      }
    }

    renderList("");
    searchInput.addEventListener("input", () => renderList(searchInput.value));
    setTimeout(() => searchInput.focus(), 50);
  });
}

export function showPortSelectDialog(
  ports: PortSelectOption[],
): Promise<PortSelectOption | null> {
  ensureDialogStyles();

  return new Promise((resolve) => {
    const { dialog, close } = openOverlay();
    dialog.style.minWidth = "360px";
    dialog.style.maxWidth = "500px";

    const title = document.createElement("h3");
    title.textContent = "选择绑定端口";
    dialog.appendChild(title);

    const hint = document.createElement("p");
    hint.style.cssText = "font-size:12px;color:var(--text-muted);margin:0 0 12px;";
    hint.textContent = "新建节点有多个兼容端口，请选择要连接的端口：";
    dialog.appendChild(hint);

    const groups: Record<"input" | "output", PortSelectOption[]> = { input: [], output: [] };
    for (const p of ports) {
      (p.isInput ? groups.input : groups.output).push(p);
    }

    const renderGroup = (label: string, items: PortSelectOption[]) => {
      if (items.length === 0) return;
      const sec = document.createElement("div");
      sec.className = "zh-section-label";
      sec.textContent = label;
      dialog.appendChild(sec);

      for (const p of items) {
        const row = document.createElement("div");
        row.style.cssText =
          "display:flex;align-items:center;gap:10px;padding:8px 12px;border:1px solid var(--border);" +
          "border-radius:6px;cursor:pointer;margin-bottom:6px;background:transparent;transition:background 0.1s, border-color 0.1s;";
        row.addEventListener("mouseenter", () => {
          row.style.background = "var(--btn-hover)";
          row.style.borderColor = "var(--link)";
        });
        row.addEventListener("mouseleave", () => {
          row.style.background = "transparent";
          row.style.borderColor = "var(--border)";
        });
        row.innerHTML = `
          <span style="font-family:monospace;font-size:13px;flex:1;color:var(--text)">${p.portName}</span>
          <span style="display:inline-block;font-size:10px;padding:2px 8px;border-radius:12px;
            background:var(--tab-inactive);border:1px solid var(--border);color:var(--link);
            font-family:monospace">${p.dataType}</span>
        `;
        row.addEventListener("click", () => {
          resolve(p);
          close();
        });
        dialog.appendChild(row);
      }
    };

    renderGroup("输入端口", groups.input);
    renderGroup("输出端口", groups.output);

    const btns = document.createElement("div");
    btns.className = "zh-buttons";
    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => {
      close();
      resolve(null);
    });
    btns.appendChild(cancelBtn);
    dialog.appendChild(btns);
  });
}
