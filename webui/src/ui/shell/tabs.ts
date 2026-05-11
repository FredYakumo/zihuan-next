export interface TabInfo {
  id: string;
  name: string;
  dirty: boolean;
  isWorkflowSet: boolean;
  workflowPath?: string;
  fileHandle?: FileSystemFileHandle;
}

export function updateTabs(
  tabs: TabInfo[],
  activeId: string | null,
  onSwitch: (id: string) => void,
  onClose: (id: string) => void,
  onNew: () => void,
): void {
  const bar = document.getElementById("graph-tabs");
  if (!bar) return;
  bar.innerHTML = "";

  for (const tab of tabs) {
    const el = document.createElement("div");
    el.className = "tab" + (tab.id === activeId ? " active" : "");
    el.title = tab.name;

    const label = document.createElement("span");
    const displayName = tab.name + (tab.isWorkflowSet ? " [工作流集]" : "");
    label.textContent = (tab.dirty ? "* " : "") + displayName;
    el.appendChild(label);

    const closeBtn = document.createElement("button");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      onClose(tab.id);
    });
    el.appendChild(closeBtn);

    el.addEventListener("click", () => onSwitch(tab.id));
    bar.appendChild(el);
  }

  const newBtn = document.createElement("button");
  newBtn.className = "new-tab";
  newBtn.textContent = "+";
  newBtn.title = "新建节点图";
  newBtn.addEventListener("click", onNew);
  bar.appendChild(newBtn);
}
