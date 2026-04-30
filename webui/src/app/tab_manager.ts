import { updateTabs, type TabInfo } from "../ui/shell/index";

export interface TabManagerOptions {
  onSwitchTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onCreateTab: () => void;
}

export class TabManager {
  private tabList: TabInfo[] = [];
  private activeTabId: string | null = null;

  constructor(private readonly options: TabManagerOptions) {}

  getTabs(): TabInfo[] {
    return this.tabList;
  }

  getActiveTabId(): string | null {
    return this.activeTabId;
  }

  getActiveTab(): TabInfo | undefined {
    return this.tabList.find((tab) => tab.id === this.activeTabId);
  }

  findTab(id: string): TabInfo | undefined {
    return this.tabList.find((tab) => tab.id === id);
  }

  setActiveTabId(id: string | null): void {
    this.activeTabId = id;
    this.render();
  }

  async openTab(id: string, name: string, dirty = false, isWorkflowSet = false): Promise<void> {
    const existing = this.tabList.findIndex((tab) => tab.id === id);
    if (existing !== -1) {
      this.tabList[existing].name = name;
      this.tabList[existing].dirty = dirty;
      this.tabList[existing].isWorkflowSet = isWorkflowSet;
    } else {
      this.tabList.push({ id, name, dirty, isWorkflowSet });
    }
    this.activeTabId = id;
    this.render();
  }

  removeTab(id: string): { removed: TabInfo | null; index: number } {
    const index = this.tabList.findIndex((tab) => tab.id === id);
    if (index === -1) return { removed: null, index: -1 };
    const [removed] = this.tabList.splice(index, 1);
    if (this.activeTabId === id) {
      this.activeTabId = null;
    }
    this.render();
    return { removed: removed ?? null, index };
  }

  getNextTabAfterRemoval(index: number): TabInfo | undefined {
    if (this.tabList.length === 0) return undefined;
    return this.tabList[Math.min(index, this.tabList.length - 1)];
  }

  setTabDirty(id: string, dirty: boolean): void {
    const tab = this.findTab(id);
    if (!tab) return;
    tab.dirty = dirty;
    this.render();
  }

  updateTab(id: string, updates: Partial<TabInfo>): void {
    const tab = this.findTab(id);
    if (!tab) return;
    Object.assign(tab, updates);
    this.render();
  }

  clearAllTabs(): void {
    this.tabList = [];
    this.activeTabId = null;
    this.render();
  }

  hasDirtyTabs(): boolean {
    return this.tabList.some((tab) => tab.dirty);
  }

  render(): void {
    updateTabs(
      this.tabList,
      this.activeTabId,
      this.options.onSwitchTab,
      this.options.onCloseTab,
      this.options.onCreateTab,
    );

    const activeTab = this.getActiveTab();
    if (activeTab) {
      const suffix = activeTab.isWorkflowSet ? " [工作流集]" : "";
      document.title = `${activeTab.name}${suffix} — Zihuan Next`;
    } else {
      document.title = "Zihuan Next — Node Graph Editor";
    }
  }
}
