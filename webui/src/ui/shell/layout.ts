import type { NodeTypeInfo } from "../../api/types";

export function buildDOM(): {
  toolbar: HTMLElement;
  tabsBar: HTMLElement;
  breadcrumb: HTMLElement;
  sidebar: HTMLElement;
  canvasContainer: HTMLElement;
  canvasEl: HTMLCanvasElement;
  backArrow: HTMLElement;
} {
  const app = document.getElementById("app")!;
  app.style.flexDirection = "column";

  const toolbar = document.createElement("div");
  toolbar.id = "toolbar";
  toolbar.innerHTML = `<span class="title">Zihuan Next</span>`;

  const tabsBar = document.createElement("div");
  tabsBar.id = "graph-tabs";

  const breadcrumb = document.createElement("div");
  breadcrumb.id = "breadcrumb";
  breadcrumb.className = "hidden";

  const main = document.createElement("div");
  main.id = "main";

  const sidebar = document.createElement("div");
  sidebar.id = "sidebar";

  const canvasContainer = document.createElement("div");
  canvasContainer.id = "canvas-container";

  const canvasEl = document.createElement("canvas");
  canvasEl.id = "graph-canvas";
  canvasContainer.appendChild(canvasEl);

  const backArrow = document.createElement("div");
  backArrow.id = "canvas-back-arrow";
  const backArrowBtn = document.createElement("button");
  backArrowBtn.textContent = "← 返回";
  backArrow.appendChild(backArrowBtn);
  canvasContainer.appendChild(backArrow);

  main.appendChild(sidebar);
  main.appendChild(canvasContainer);

  app.appendChild(toolbar);
  app.appendChild(tabsBar);
  app.appendChild(breadcrumb);
  app.appendChild(main);

  return { toolbar, tabsBar, breadcrumb, sidebar, canvasContainer, canvasEl, backArrow };
}

export function updateBreadcrumb(
  labels: string[],
  onNavigateTo?: (depth: number) => void,
): void {
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
  if (onNavigateTo) {
    root.addEventListener("click", () => onNavigateTo(0));
  }
  breadcrumb.appendChild(root);

  for (let i = 0; i < labels.length; i++) {
    const sep = document.createElement("span");
    sep.className = "bc-sep";
    sep.textContent = "›";
    breadcrumb.appendChild(sep);

    const item = document.createElement("span");
    const isLast = i === labels.length - 1;
    item.className = isLast ? "bc-item" : "bc-item bc-clickable";
    item.textContent = labels[i];
    if (!isLast && onNavigateTo) {
      item.addEventListener("click", () => onNavigateTo(i + 1));
    }
    breadcrumb.appendChild(item);
  }
}

export function buildSidebar(
  sidebar: HTMLElement,
  nodeTypes: NodeTypeInfo[],
  onDrop: (typeId: string, x: number, y: number) => void,
  canvasContainer: HTMLElement,
): void {
  sidebar.innerHTML = "";

  const categories = new Map<string, NodeTypeInfo[]>();
  for (const nt of nodeTypes) {
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

  canvasContainer.addEventListener("dragover", (e) => e.preventDefault());
  canvasContainer.addEventListener("drop", (e) => {
    e.preventDefault();
    const typeId = e.dataTransfer?.getData("application/zihuan-node-type");
    if (!typeId) return;
    const rect = canvasContainer.getBoundingClientRect();
    onDrop(typeId, e.clientX - rect.left, e.clientY - rect.top);
  });
}
