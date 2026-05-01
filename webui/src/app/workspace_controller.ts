import { fileIO, graphs } from "../api/client";
import type { NodeGraphDefinition } from "../api/types";
import type { ZihuanCanvas } from "../graph/canvas";
import type { TabManager } from "./tab_manager";
import { loadWorkspaceState, saveWorkspaceState } from "./workspace";

export interface WorkspaceControllerOptions {
  canvas: ZihuanCanvas;
  tabs: TabManager;
  isSessionRunning: (sessionId: string | null) => boolean;
  updateRunButton: (isRunning: boolean) => void;
  createNewTab: () => Promise<void>;
}

export class WorkspaceController {
  private skipWorkspaceSave = false;

  constructor(private readonly options: WorkspaceControllerOptions) {}

  async persistWorkspaceState(): Promise<void> {
    await saveWorkspaceState({
      tabList: this.options.tabs.getTabs(),
      activeTabId: this.options.tabs.getActiveTabId(),
      skipWorkspaceSave: this.skipWorkspaceSave,
      getViewport: () => this.options.canvas.getCanvasViewport(),
      getGraphJson: (sessionId) => graphs.get(sessionId),
    });
  }

  async restoreOrCreateTabs(): Promise<void> {
    const state = loadWorkspaceState();
    let lastCanvasState: { offset: [number, number]; scale: number } | null = null;

    this.skipWorkspaceSave = true;
    try {
      if (state && state.tabs.length > 0) {
        for (let i = 0; i < state.tabs.length; i++) {
          const tab = state.tabs[i];

          if (tab.isWorkflowSet && tab.workflowPath) {
            try {
              const openResult = await fileIO.open(tab.workflowPath);
              await this.options.tabs.openTab(openResult.session_id, tab.name, false, true);
              await this.options.canvas.loadExternalSession(openResult.session_id);

              if (i === state.activeTabIndex && tab.canvasOffset && tab.canvasScale) {
                lastCanvasState = { offset: tab.canvasOffset, scale: tab.canvasScale };
              }
            } catch {
              console.warn(`无法恢复 ${tab.workflowPath}，跳过`);
            }
          } else if (tab.workflowJSON) {
            try {
              const newTab = await graphs.create();
              await graphs.put(newTab.id, tab.workflowJSON as NodeGraphDefinition);
              await this.options.tabs.openTab(newTab.id, tab.name, false, false);
              await this.options.canvas.loadExternalSession(newTab.id);

              if (i === state.activeTabIndex && tab.canvasOffset && tab.canvasScale) {
                lastCanvasState = { offset: tab.canvasOffset, scale: tab.canvasScale };
              }
            } catch {
              console.warn(`无法恢复工作流 ${tab.name}，跳过`);
            }
          }
        }

        const allTabs = this.options.tabs.getTabs();
        if (allTabs.length > 0 && state.activeTabIndex < allTabs.length) {
          const activeTab = allTabs[state.activeTabIndex];
          if (activeTab) {
            this.options.tabs.setActiveTabId(activeTab.id);
            await this.options.canvas.loadExternalSession(activeTab.id);
            this.options.updateRunButton(this.options.isSessionRunning(activeTab.id));
            if (lastCanvasState) {
              this.options.canvas.setCanvasViewport(lastCanvasState.offset, lastCanvasState.scale);
            }
          }
        }
      }
    } finally {
      this.skipWorkspaceSave = false;
    }

    if (this.options.tabs.getTabs().length === 0) {
      await this.options.createNewTab();
    }
  }
}
