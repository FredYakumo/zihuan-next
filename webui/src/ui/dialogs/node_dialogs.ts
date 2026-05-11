import type { NodeTypeInfo } from "../../api/types";
import { ensureDialogStyles, openOverlay } from "./base";
import type { PortConnInfo } from "./types";
import "./node_dialogs.css";

function ensureNodeInfoStyles(): void {
  // Styles are injected via CSS import (node_dialogs.css)
}

function buildPortSection(ports: PortConnInfo[], showConnections: boolean): HTMLElement {
  const col = document.createElement("div");
  if (ports.length === 0) {
    const empty = document.createElement("div");
    empty.className = "zh-ni-empty";
    empty.textContent = "（无端口）";
    col.appendChild(empty);
    return col;
  }
  for (const p of ports) {
    const portEl = document.createElement("div");
    portEl.className = "zh-ni-port";

    const top = document.createElement("div");
    top.className = "zh-ni-port-top";

    const nameEl = document.createElement("span");
    nameEl.className = "zh-ni-port-name";
    nameEl.textContent = p.portName;
    top.appendChild(nameEl);

    const badge = document.createElement("span");
    badge.className = "zh-ni-type-badge";
    badge.textContent = p.dataType;
    top.appendChild(badge);

    if (p.required) {
      const req = document.createElement("span");
      req.className = "zh-ni-required";
      req.textContent = "必填";
      top.appendChild(req);
    }

    portEl.appendChild(top);

    if (p.description) {
      const desc = document.createElement("div");
      desc.className = "zh-ni-port-desc";
      desc.textContent = p.description;
      portEl.appendChild(desc);
    }

    if (showConnections && p.connectedTo.length > 0) {
      for (const conn of p.connectedTo) {
        const connEl = document.createElement("div");
        connEl.className = "zh-ni-conn";
        connEl.textContent = `→ ${conn.nodeName} : ${conn.portName}`;
        portEl.appendChild(connEl);
      }
    }

    col.appendChild(portEl);
  }
  return col;
}

export function showAddNodeDialog(nodeTypes: NodeTypeInfo[]): Promise<string | null> {
  ensureDialogStyles();
  ensureNodeInfoStyles();

  // Styles are injected via CSS import (node_dialogs.css)

  const visibleTypes = nodeTypes.filter((n) => n.category !== "内部");
  const cats = ["全部", ...Array.from(new Set(visibleTypes.map((n) => n.category)))];
  const supportsHoverPreview = window.matchMedia("(hover: hover) and (pointer: fine)").matches;

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-dialog zh-an-dialog";

    const leftPane = document.createElement("div");
    leftPane.className = "zh-an-left";

    const titleEl = document.createElement("h3");
    titleEl.textContent = "选择节点类型";
    titleEl.style.cssText = "margin: 0 0 12px; font-size: 15px;";
    leftPane.appendChild(titleEl);

    const searchInput = document.createElement("input");
    searchInput.type = "text";
    searchInput.className = "zh-an-search";
    searchInput.placeholder = "输入名称、类型、分类或描述…";
    leftPane.appendChild(searchInput);

    const tabsRow = document.createElement("div");
    tabsRow.className = "zh-an-tabs";
    leftPane.appendChild(tabsRow);

    const listEl = document.createElement("div");
    listEl.className = "zh-an-list";
    leftPane.appendChild(listEl);

    dialog.appendChild(leftPane);

    const rightPane = document.createElement("div");
    rightPane.className = "zh-an-right";

    const placeholder = document.createElement("div");
    placeholder.className = "zh-an-detail-placeholder";
    placeholder.textContent = supportsHoverPreview
      ? "悬浮在节点上以查看说明"
      : "点击节点即可添加";
    rightPane.appendChild(placeholder);

    dialog.appendChild(rightPane);

    const footer = document.createElement("div");
    footer.style.cssText = "display:flex;justify-content:flex-end;padding-top:12px;margin-top:auto;";
    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => {
      document.body.removeChild(overlay);
      resolve(null);
    });
    footer.appendChild(cancelBtn);
    leftPane.appendChild(footer);

    overlay.appendChild(dialog);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) {
        document.body.removeChild(overlay);
        resolve(null);
      }
    });

    let activeCategory = "全部";
    let searchText = "";
    let currentDetailTypeId: string | null = null;

    const updateActiveItem = () => {
      const items = listEl.querySelectorAll<HTMLElement>(".zh-an-item");
      for (const item of items) {
        item.classList.toggle("active", item.dataset.typeId === currentDetailTypeId);
      }
    };

    function renderDetail(nt: NodeTypeInfo): void {
      if (currentDetailTypeId === nt.type_id) return;
      currentDetailTypeId = nt.type_id;
      rightPane.innerHTML = "";

      const nameEl = document.createElement("div");
      nameEl.className = "zh-an-detail-title";
      nameEl.textContent = nt.display_name;
      rightPane.appendChild(nameEl);

      const catBadge = document.createElement("span");
      catBadge.className = "zh-an-badge";
      catBadge.style.display = "inline-block";
      catBadge.style.marginBottom = "8px";
      catBadge.textContent = nt.category;
      rightPane.appendChild(catBadge);

      if (nt.description) {
        const descEl = document.createElement("div");
        descEl.className = "zh-an-detail-desc";
        descEl.textContent = nt.description;
        rightPane.appendChild(descEl);
      }

      const makeConfigFields = () => {
        if (nt.config_fields.length === 0) return;
        const section = document.createElement("div");
        section.className = "zh-an-detail-section";
        section.textContent = "配置项";
        rightPane.appendChild(section);

        for (const field of nt.config_fields) {
          const fieldDiv = document.createElement("div");
          fieldDiv.className = "zh-an-detail-port";

          const top = document.createElement("div");
          top.className = "zh-an-detail-port-top";

          const fieldName = document.createElement("span");
          fieldName.className = "zh-an-detail-port-name";
          fieldName.textContent = field.key;
          top.appendChild(fieldName);

          const typeBadge = document.createElement("span");
          typeBadge.className = "zh-ni-type-badge";
          typeBadge.textContent = field.data_type;
          top.appendChild(typeBadge);

          if (field.required) {
            const req = document.createElement("span");
            req.className = "zh-ni-required";
            req.textContent = "必填";
            top.appendChild(req);
          }

          fieldDiv.appendChild(top);

          const widgetHint = document.createElement("div");
          widgetHint.className = "zh-an-detail-port-desc";
          widgetHint.textContent = field.widget === "connection_select"
            ? "通过连接配置选择器填写"
            : field.widget === "agent_llm_kind_select"
              ? "通过 Agent LLM 类型选择器填写"
            : field.widget === "llm_ref_select"
              ? "通过 LLM 配置选择器填写"
              : field.widget === "active_bot_adapter_select"
                ? "通过已激活 Bot Adapter 选择器填写"
                : `通过 ${field.widget} 填写`;
          fieldDiv.appendChild(widgetHint);

          if (field.description) {
            const desc = document.createElement("div");
            desc.className = "zh-an-detail-port-desc";
            desc.textContent = field.description;
            fieldDiv.appendChild(desc);
          }

          rightPane.appendChild(fieldDiv);
        }
      };

      const makePorts = (ports: NodeTypeInfo["input_ports"], sectionLabel: string) => {
        if (ports.length === 0) return;
        const section = document.createElement("div");
        section.className = "zh-an-detail-section";
        section.textContent = sectionLabel;
        rightPane.appendChild(section);
        for (const p of ports) {
          const portDiv = document.createElement("div");
          portDiv.className = "zh-an-detail-port";

          const top = document.createElement("div");
          top.className = "zh-an-detail-port-top";

          const pname = document.createElement("span");
          pname.className = "zh-an-detail-port-name";
          pname.textContent = p.name;
          top.appendChild(pname);

          const dt = typeof p.data_type === "string" ? p.data_type : JSON.stringify(p.data_type);
          const typeBadge = document.createElement("span");
          typeBadge.className = "zh-ni-type-badge";
          typeBadge.textContent = dt;
          top.appendChild(typeBadge);

          if (p.required) {
            const req = document.createElement("span");
            req.className = "zh-ni-required";
            req.textContent = "必填";
            top.appendChild(req);
          }

          portDiv.appendChild(top);

          if (p.description) {
            const desc = document.createElement("div");
            desc.className = "zh-an-detail-port-desc";
            desc.textContent = p.description;
            portDiv.appendChild(desc);
          }

          rightPane.appendChild(portDiv);
        }
      };

      makeConfigFields();
      makePorts(nt.input_ports, "输入端口");
      makePorts(nt.output_ports, "输出端口");
      updateActiveItem();
    }

    function applyFilter() {
      const q = searchText.toLowerCase();
      return visibleTypes.filter((n) => {
        const catMatch = activeCategory === "全部" || n.category === activeCategory;
        if (!catMatch) return false;
        if (!q) return true;
        return (
          n.display_name.toLowerCase().includes(q) ||
          n.type_id.toLowerCase().includes(q) ||
          n.category.toLowerCase().includes(q) ||
          n.description.toLowerCase().includes(q)
        );
      });
    }

    function renderList() {
      listEl.innerHTML = "";
      const filtered = applyFilter();
      if (filtered.length === 0) {
        const empty = document.createElement("div");
        empty.className = "zh-an-empty";
        empty.textContent = "无匹配节点";
        listEl.appendChild(empty);
        return;
      }
      for (const nt of filtered) {
        const item = document.createElement("div");
        item.className = "zh-an-item";
        item.dataset.typeId = nt.type_id;

        const top = document.createElement("div");
        top.className = "zh-an-item-top";

        const name = document.createElement("span");
        name.className = "zh-an-name";
        name.textContent = nt.display_name;
        top.appendChild(name);

        const badge = document.createElement("span");
        badge.className = "zh-an-badge";
        badge.textContent = nt.category;
        top.appendChild(badge);

        item.appendChild(top);

        if (nt.description) {
          const desc = document.createElement("div");
          desc.className = "zh-an-desc";
          desc.textContent = nt.description;
          item.appendChild(desc);
        }

        if (supportsHoverPreview) {
          item.addEventListener("mouseenter", () => renderDetail(nt));
        }
        item.addEventListener("focus", () => renderDetail(nt));
        item.addEventListener("click", () => {
          document.body.removeChild(overlay);
          resolve(nt.type_id);
        });

        listEl.appendChild(item);
      }
      updateActiveItem();
    }

    function renderTabs() {
      tabsRow.innerHTML = "";
      for (const cat of cats) {
        const tab = document.createElement("button");
        tab.className = "zh-an-tab" + (cat === activeCategory ? " active" : "");
        tab.textContent = cat;
        tab.addEventListener("click", () => {
          activeCategory = cat;
          renderTabs();
          renderList();
        });
        tabsRow.appendChild(tab);
      }
    }

    searchInput.addEventListener("input", () => {
      searchText = searchInput.value;
      renderList();
    });

    renderTabs();
    renderList();

    document.body.appendChild(overlay);
    setTimeout(() => searchInput.focus(), 0);
  });
}

export function showNodeInfoDialog(
  info: NodeTypeInfo,
  inputConns: PortConnInfo[],
  outputConns: PortConnInfo[],
): void {
  ensureDialogStyles();
  ensureNodeInfoStyles();
  const { dialog, close } = openOverlay();
  dialog.className = "zh-dialog zh-ni-dialog";

  const header = document.createElement("div");
  header.className = "zh-ni-header";

  const titleEl = document.createElement("h3");
  titleEl.className = "zh-ni-title";
  titleEl.textContent = info.display_name;
  header.appendChild(titleEl);

  if (info.description) {
    const desc = document.createElement("p");
    desc.className = "zh-ni-desc";
    desc.textContent = info.description;
    header.appendChild(desc);
  }

  dialog.appendChild(header);

  const cols = document.createElement("div");
  cols.className = "zh-ni-cols";

  const inCol = document.createElement("div");
  const inTitle = document.createElement("div");
  inTitle.className = "zh-ni-col-title";
  inTitle.textContent = "输入端口";
  inCol.appendChild(inTitle);
  inCol.appendChild(buildPortSection(inputConns, true));
  cols.appendChild(inCol);

  const outCol = document.createElement("div");
  const outTitle = document.createElement("div");
  outTitle.className = "zh-ni-col-title";
  outTitle.textContent = "输出端口";
  outCol.appendChild(outTitle);
  outCol.appendChild(buildPortSection(outputConns, true));
  cols.appendChild(outCol);

  dialog.appendChild(cols);

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const closeBtn = document.createElement("button");
  closeBtn.textContent = "关闭";
  closeBtn.className = "primary";
  closeBtn.addEventListener("click", close);
  footer.appendChild(closeBtn);
  dialog.appendChild(footer);

  setTimeout(() => closeBtn.focus(), 0);
}
