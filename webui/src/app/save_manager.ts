import { graphs, workflows as workflowsApi } from "../api/client";
import type { ZihuanCanvas } from "../graph/canvas";
import { showErrorDialog, showSaveAsDialog } from "../ui/dialogs/index";
import type { TabManager } from "./tab_manager";
import { tabNameFrom } from "./workspace";

export interface SaveManagerOptions {
  canvas: ZihuanCanvas;
  tabs: TabManager;
  persistWorkspace: () => Promise<void>;
}

export class SaveManager {
  constructor(private readonly options: SaveManagerOptions) {}

  async saveCurrent(): Promise<void> {
    const sid = await this.flushBeforeSave(true);
    if (!sid) return;

    const currentTab = this.options.tabs.findTab(sid);
    if (currentTab?.fileHandle) {
      try {
        await this.writeViaFileHandle(sid, currentTab.fileHandle);
        this.options.tabs.setTabDirty(sid, false);
      } catch (e) {
        showErrorDialog(`保存失败: ${(e as Error).message}`);
      }
      return;
    }

    if (currentTab?.isWorkflowSet) {
      try {
        const result = await workflowsApi.save(sid, currentTab.name);
        const displayName = tabNameFrom(result.path, currentTab.name);
        this.options.tabs.updateTab(sid, {
          name: displayName,
          dirty: false,
          isWorkflowSet: true,
          workflowPath: result.path,
        });
        await this.options.persistWorkspace();
      } catch (e) {
        showErrorDialog(`保存失败: ${(e as Error).message}`);
      }
      return;
    }

    try {
      await graphs.saveFile(sid);
      this.options.tabs.setTabDirty(sid, false);
    } catch {
      await this.saveCurrentAs();
    }
  }

  async saveCurrentAs(): Promise<void> {
    const sid = await this.flushBeforeSave(false);
    if (!sid) return;

    const currentTab = this.options.tabs.findTab(sid);
    const defaultName = currentTab?.name ?? "untitled";
    const choice = await showSaveAsDialog(defaultName);
    if (!choice) return;

    if (choice === "local") {
      const url = graphs.downloadUrl(sid);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${defaultName}.json`;
      a.click();
      if (currentTab) {
        this.options.tabs.updateTab(sid, { isWorkflowSet: false });
      }
      return;
    }

    const name = prompt("保存到 workflow_set/ 目录，文件名:", defaultName);
    if (!name) return;
    try {
      const result = await workflowsApi.save(sid, name);
      const displayName = tabNameFrom(result.path, name);
      this.options.tabs.updateTab(sid, {
        name: displayName,
        isWorkflowSet: true,
        dirty: false,
        workflowPath: result.path,
      });
    } catch (e) {
      showErrorDialog(`保存失败: ${(e as Error).message}`);
    }
  }

  async saveCurrentToWorkflows(): Promise<void> {
    const sid = await this.flushBeforeSave(false);
    if (!sid) return;

    const currentTab = this.options.tabs.findTab(sid);
    const defaultName = currentTab?.name ?? "untitled";
    const name = prompt("保存到 workflow_set/ 目录，文件名:", defaultName);
    if (!name) return;

    try {
      const result = await workflowsApi.save(sid, name);
      const displayName = tabNameFrom(result.path, name);
      this.options.tabs.updateTab(sid, {
        name: displayName,
        isWorkflowSet: true,
        dirty: false,
        workflowPath: result.path,
      });
      await this.options.persistWorkspace();
    } catch (e) {
      showErrorDialog(`保存失败: ${(e as Error).message}`);
    }
  }

  async autoSaveDirtyTabs(): Promise<void> {
    await this.options.canvas.syncInlineWidgetValues();
    await this.options.canvas.flushPendingWidgetMutations();
    for (const tab of this.options.tabs.getTabs()) {
      if (!tab.dirty) continue;
      try {
        if (this.options.canvas.isInSubgraph && tab.id === this.options.canvas.rootSessionId) {
          try {
            await this.options.canvas.flushSubgraphToRoot();
          } catch {}
        }
        if (tab.fileHandle) {
          await this.writeViaFileHandle(tab.id, tab.fileHandle);
          this.options.tabs.setTabDirty(tab.id, false);
        } else if (tab.isWorkflowSet) {
          const result = await workflowsApi.save(tab.id, tab.name);
          const displayName = tabNameFrom(result.path, tab.name);
          this.options.tabs.updateTab(tab.id, {
            name: displayName,
            dirty: false,
            isWorkflowSet: true,
            workflowPath: result.path,
          });
        } else {
          await graphs.saveFile(tab.id);
          this.options.tabs.setTabDirty(tab.id, false);
        }
      } catch {
        // Skip tabs without a writable path.
      }
    }
  }

  private async flushBeforeSave(strictFlush: boolean): Promise<string | null> {
    await this.options.canvas.syncInlineWidgetValues();
    await this.options.canvas.flushPendingWidgetMutations();
    await this.options.canvas.syncPositions();
    if (this.options.canvas.isInSubgraph) {
      try {
        await this.options.canvas.flushSubgraphToRoot();
      } catch (e) {
        if (strictFlush) {
          showErrorDialog(`保存前同步子图失败: ${(e as Error).message}`);
          return null;
        }
      }
    }
    const sid = this.options.canvas.rootSessionId;
    if (!sid) {
      showErrorDialog("请先打开一个节点图");
      return null;
    }
    return sid;
  }

  private async writeViaFileHandle(sid: string, handle: FileSystemFileHandle): Promise<void> {
    const resp = await fetch(graphs.downloadUrl(sid));
    const json = await resp.text();
    const writable = await handle.createWritable();
    await writable.write(json);
    await writable.close();
  }
}
